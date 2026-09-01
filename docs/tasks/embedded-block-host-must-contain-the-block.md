---
title: The host entity of an embedded block has to CONTAIN the block — and the grammar needs to be able to declare its extent
status: in-progress
created: 2026-08-19
updated: 2026-08-19
tags: [ast, embedded, grammar, dml]
---

# The host entity of an embedded block has to CONTAIN the block

## Objective

The attribution of an embedded block to the host entity (`attributeToHostEntity` +
`hostEntityAt`, `internal/ast/treesitter_embedded.go`) hands back **the sibling above**, not
the one that hosts it. In an orchestration XML with SQL inside a `<value>`, the DML edge is born
in the `Element` of the `<key>` that precedes the value — a tag name, with no identity, and the same node
for every block in the file, because the entity uid is `(path, name)`.

Practical consequence, reported by a consumer project: the question "**which unit**
of this flow writes to this table?" still has no useful answer. Before the attribution the
answer was the file; now it is a `<key>` — more precise in form and equally useless in
content.

They are two independent defects, and the second is the one that blocks the consumer:

1. **An off-by-one error in the position of the block.** The caller passes `innerOffset` as
   "the line of the block". `innerOffset` is the OFFSET — how much to add to a 1-based line of the
   sub-parse —, so it equals `primeiraLinha - 1`. `hostEntityAt` receives the line BEFORE the
   block and picks whatever is there: in indented XML, the `<key>` of the `<entry>`.

2. **An XML entity has the span of the TAG, not of the element.** The `endLine` of a
   tree-sitter entity is the end of the PARENT of the captured node (`treesitter_adapter.go`), and the parent of
   `(STag (Name) @name)` is the start tag. Every `Element` of this corpus has
   `end_line == line_number` — measured on the live graph: no node in the file satisfies
   `line_number <= L <= end_line` for a line L in the middle of a block. In other words: **no
   custom grammar can declare the meaningful unit** (the step, the job, the
   processor) with enough extent to host the block. Fixing item 1 alone swaps
   `<key>` for `<value>` — the block's own wrapper, which says as much as the text of the
   statement.

## Reasoning

What has already been verified and is NOT repeated here (it comes from the consumer, measured on the live graph on
2026-08-19): the DML edges of embedded SQL reach the tables, the `File → File`
self-loops are gone, and the language stamp of the block is correct. What remains is
granularity — who the edge LEAVES from.

Memories consulted before designing: *Embedded block: entity, call and reference
carry the PRODUCING language* (which records this pendency as open), *A reference with no
entity around it goes to the File*, *The engine knows FORMATS (xml, sql, json), never
TOOLS* and *A fixed list of labels in the code breaks extensibility — what a
grammar produces, the grammar declares in its YAML*. The last one is the design criterion
used below.

Also relevant, and it corrects a premise of the consumer's report: **`merge: true` exists**
since 2026-08-12 (`docs/tasks/grammar-override-via-config-and-merge.md` →
`grammar-override-via-config-and-merge.md`). A project grammar no longer needs
to be an entire copy of the runtime one, so "adding a new query without freezing the version
shipped" is already possible and is not part of this work.

## Justification — why this way, and not the other ways

**Item 1** has no alternative: it is an arithmetic error. The existing unit test
(`TestEmbeddedBlockIsAttributedToItsHostEntity`) passes an ABSOLUTE line and asserts the
right behaviour; the one who gets it wrong is the caller, which no test exercised. The fix comes
with a test through the real parse path.

**Item 2** — three designs considered:

| Design | Why not |
|---|---|
| Make every XML entity span the whole element (changing `context_name_paths`/the `endLine` calculation of the shipped grammar) | It changes the span of EVERY entity of EVERY data grammar — `ast_source --entity`, dedup, host — to solve one case. And the `<value>` would then contain the block, becoming the host: the wrapper again. |
| Prefer labels declared as "hostable" (`host_labels: [...]`) | Discarded in the first round on YAGNI grounds — with a real span, the containment rule already eliminates the wrapper. **It came back, and was implemented**, when the second case appeared: a block that lives in an ATTRIBUTE of the unit's own tag (see T7). |
| Pick the host by the ancestor in tree-sitter, not by lines | It would require linking entity↔node, which the cache does not keep; and the attribution runs after the parse of the block, not during the sweep of the host. Much more mechanism for the same result. |

Chosen: **`span_capture`** — an entity query can name the capture whose node
DELIMITS the entity, instead of inheriting the parent of the name's node. It is additive (absent = today),
declarative (the grammar says what it produces) and it is the only way for a project grammar
to express "this unit goes from `<processors>` to `</processors>`, and is named by the text
of the `<name>` child" — whose name, in that format, appears AFTER the block, so not even the
beginning of the unit can be inferred from the position of the name.

And **containment**: the host becomes the innermost entity that contains the WHOLE block,
not the one that crosses its first line. It is the same reason content labels are already
skipped — attributing the statement to the text of the statement says nothing —, and it is what
automatically disqualifies the wrapper whose span is only the tag.

## Plan & Task Breakdown

- [x] **T1 — the position of the block becomes the real interval** — Spec: in
  `parseEmbeddedBody` (`internal/ast/treesitter_embedded.go`), compute the absolute first and last
  line of the text node and pass both to `attributeToHostEntity`. Done when
  a real parse of an XML with `<key>` on the line above the `<value>` attributes to the element that
  contains the block, never to the sibling above. Invariant: `innerOffset` remains the
  offset of the sub-parse — do not change what `shiftParsedLines` receives.
- [x] **T2 — `hostEntityAt` requires containment of the whole block** — Spec: the signature becomes
  `(pf, startLine, endLine)`; a candidate requires `e.Line <= startLine && e.EndLine >= endLine`;
  content labels keep being skipped; the deterministic tie-break is preserved. Done when
  an entity that ends inside the block stops being a host.
- [x] **T3 — `span_capture` in an entity query** — Spec: a `SpanCapture` field in
  `ExternalQueryDef` (`span_capture`), the index compiled in `compiledQuery`, and in
  `treesitter_adapter.go` the pair `(startLine, endLine)` comes from the node of that capture when
  declared. It does NOT change `entitySource` nor the complexity (the export verdict reads the
  declaration, not the whole element). Done when a query with `span_capture` produces an
  entity with the span of the captured node and, without it, nothing changes.
- [x] **T4 — tests** — Spec: `internal/ast/embedded_lang_resolution_test.go` gains the case
  through the real parse path (the one that would have caught the off-by-one error) and the containment one;
  `internal/ast/` gains the `span_capture` case (with and without). Run all of `internal/ast`.
- [x] **T5 — grammar of the consumer project** — Spec: outside this repository, in the
  consumer's `ast.queries_dir`, add the query for the named unit with
  `span_capture`. Done when, on its live graph, the `INSERTS` edge of the target table leaves
  the named unit, and `forms/`/`reports/` are still extracted.
- [x] **T6 — documentation** — Spec: `docs/specs/ast_module.md` (new field in the table of
  query fields + example), `docs/specs/embedded_language_parsing.md` (section on attribution
  to the host, with the containment rule and the role of `span_capture`), this log, memories.

### T7 — the block that lives in an ATTRIBUTE of the unit's tag (`host_labels`)

Discovered while extending the same work to another format of the same corpus: the PL/SQL of
a screen exported as XML lives in `<Trigger Name="POST-QUERY" TriggerText="…"/>`, with line breaks
encoded as text — so **unit and block occupy the same physical line**. The strict
containment of T2 excludes precisely the entity that should answer, and everything that contains the block is
coarser than it (the item, the data block, the form).

`EmbeddedBlock.HostLabels` (`host_labels`) solves it by declaring which labels are UNITS for
that block: it filters the candidates by label and waives the strictness. Absent, the default rule
of T2 holds. It is the same design criterion as the rest of this file — what a grammar produces,
the grammar declares — and now there are two independent cases asking for it, not one.

### T8 — `name_is_data`, and the trap of the predicate

Two defects that only show up when a grammar names entities from ATTRIBUTES:

1. **The name came with the quotes.** The `AttValue` of this grammar spans the quotes themselves, and
   `dataText` — which strips them — only runs when the query declares `value_capture` or
   `parent_capture`, which a query naming a unit has no reason to declare. The
   entity was called `"POST-QUERY"`, and no query finds it.

   The first attempt was to strip matched quotes from EVERY entity name. **An existing test
   caught it**: `TestQuotedBindingIsNotAnIdentifierReference` pins that a quoted literal
   does NOT collapse into the identifier of the same spelling — `:prop="foo"` binds the variable and
   `:prop='"foo"'` passes the string. In other words, it is not inferable from the node: only the grammar knows.
   It became `ExternalQueryDef.NameIsData` (`name_is_data`), which makes the name go through the same
   normalization as a value.

2. **`#eq?` written OUTSIDE the pattern's parenthesis does not filter anything.** `(node) @cap (#eq? …)`
   compiles as TWO patterns, and the predicate restricts the second one, which captures nothing.
   Measured with a minimal probe: on a tag with three attributes, all three values reached the
   PL/SQL parser, including one planted as bait. Inside the parenthesis, only the intended one arrives. It is not a
   defect of the engine — it is syntax that deceives, and now it is written in both specifications.

### T9 — the CALLS edge of the host unit, and the fixed list that blocked it

Verified on the consumer's live graph AFTER indexing: the DML edges left the
units (2616 SELECTS from a screen trigger label, 9043 from a record group, 4409 from a report
query), and the **CALLS ones were zero**. Two causes, one at each end:

1. `attributeToHostEntity` stamped only the NAME on the `CallSite`. A reference has its origin
   label derived from the uid at rebuild time; a CALL carries the label explicitly in
   `CallInfo.SourceType`, which comes from the context of the INNER language — empty in a loose
   statement, and `cache_convert` then assumes `Function`. Result: an edge written from
   a Function that does not exist, and discarded. Now the attribution stamps the name **and the label**.
2. `rebuild_index.go` assembled `callerSet` from a **fixed list in Go** —
   `Function, Method, Procedure, Trigger, Package, File`. Any other caller label
   was silently discarded, so the CALLS pair was never declared. It is exactly the
   anti-pattern this repository has already recorded in memory ("a fixed list of labels in the code
   breaks extensibility"). Removed: now any non-empty `SourceType` gets in, and the
   real validation stays where it always was — `canWriteCallerLabel` requires the emitted label,
   `callRelPairs` cross-checks against the existing node tables, and `callEdgeJSON` requires the caller's uid
   in that table.

Tests: `TestHostAttributionStampsTheHostLabelOnACall` (the stamp) and
`TestCallFromAHostUnitReachesTheGraph` (the declared pair, the writer's gate and the line
generated) — the latter for the same reason the equivalent DML test exists: asserting about
`pf.CallSites` does not prove an edge in the graph.

## Implementation Details

### T1 + T2 — `internal/ast/treesitter_embedded.go`

`parseEmbeddedBody` now computes the absolute interval of the body from the text node
itself, and passes it whole:

```go
blockFirst := lineOffset + int(textNode.StartPosition().Row) + 1
blockLast := lineOffset + int(textNode.EndPosition().Row) + 1
attributeToHostEntity(out, inner, blockFirst, blockLast)
```

`innerOffset` still exists and is still the offset (`blockFirst - 1`), which is
what the sub-parse and `shiftParsedLines` consume. The error was using one as the other.

`hostEntityAt(pf, startLine, endLine)` now requires
`e.Line <= startLine && e.EndLine >= endLine`, **and strict containment**: an entity whose
span COINCIDES with the block's is also discarded. An entity that starts inside the block,
or that ends before its end, stops being a candidate — including the wrapper whose span is
only the start tag.

Strict containment was not in the initial design; it came in after the measurement on the real corpus,
which showed 3 of the 28 references of a flow still leaving an `Element` named `value`.
Those are the statements written on ONE line: `<value>select …</value>` puts the start tag and the whole
block on the same line, so the span of the wrapper equals that of the block and simple containment
picked it — for the same reason the text node is skipped, it is not a unit around it.

### T3 — `span_capture`

- `ExternalQueryDef.SpanCapture` (`span_capture`), documented on the field.
- `compiledQuery.SpanIdx`, resolved with `captureIndex` like the others.
- `treesitter_adapter.go`: when `SpanIdx >= 0` and the capture has a node, `startLine`/`endLine`
  come from it; otherwise, the previous behaviour (name node → end of the parent).
- Merge: `span_capture` lives inside a query, and queries merge whole by `data_key`
  — nothing to do in `mergeQueryFile`.

### T5 — the consumer's grammar (outside this repository)

The query for the named unit uses `span_capture` over the whole element. Two of its decisions
hold as general knowledge about `span_capture`, and both were MEASURED on an
888 KB file of the corpus:

| pattern of the unit | parse cost | identity of the nodes |
|---|---|---|
| name by an unanchored child | 1.89 s | collapses: 27 of 28 references leave one node |
| name by the FIRST child, anchored | 1.65 s | 11 distinct nodes, one per unit |
| first child anchored + second child in the gap (to get a readable AND unique name) | **18.51 s** | 11 distinct nodes |

In other words: **a gap between two captured children costs ten times as much**, and it is the same cost lesson
the `embedded` block of that project had already recorded (anchoring the siblings took a pattern from
13.5 s to 327 ms). With a corpus whose largest file has 400 thousand lines, the homemade variant
is not an option — the unit is named by the child that can be anchored, and the readable name is read from
the interval of the node, which now covers the whole unit.

The collapse in the first row of the table is the `(path, name)` uid debt already recorded below,
and it stopped being theoretical: repeated unit names are the norm, not the exception, in a
tool-generated document.

## Use Cases

### UC-01: SQL embedded in an orchestration XML is attributed to the unit that hosts it
- **Actor**: the indexer (`graphit ast index`, or the daemon when it sees the file change).
- **Preconditions**: the host grammar declares an `embedded` block that matches the body; the
  grammar declares an entity query with `span_capture` covering the unit.
- **Main Flow**:
  1. `parseEmbedded` matches the body and resolves the inner language.
  2. The sub-parse produces DML references with no entity around them (`SourceName` empty).
  3. `parseEmbeddedBody` computes the absolute interval of the body.
  4. `attributeToHostEntity` asks `hostEntityAt` for the innermost entity that contains that
     whole interval, ignoring content labels.
  5. Each reference without an origin receives the name of that entity.
  6. `ConvertToCache` resolves the name to the entity's uid; the rebuild writes
     `Unidade -[:INSERTS]-> Table`.
- **Alternative Flows**:
  - The block declares its own named unit (a `create procedure` inside the value):
    `SourceName` is already filled in and the attribution does not overwrite it.
  - No entity contains the block: the origin remains the file — the answer from
    before, not an error.
- **Error Scenarios**:
  - `span_capture` names a capture absent from the pattern: `captureIndex` returns -1 and the
    span falls back to the previous behaviour (name node), without breaking the parse.
  - Two units with the SAME name in the same file collapse into one node (the uid is `(path, name)`)
    — a known limitation, recorded in Technical Debt.
- **Postconditions**: the DML edge leaves the named unit, and not the file nor a tag
  node.
- **Affected Files**: `internal/ast/treesitter_embedded.go`,
  `internal/ast/treesitter_adapter.go`, `internal/ast/query_loader.go`.

### UC-02: A grammar declares the real extent of an entity
- **Actor**: grammar author (shipped, user's or project's).
- **Preconditions**: the entity query captures, besides the name, the node that delimits the
  entity.
- **Main Flow**:
  1. The query declares `span_capture: <captura>`.
  2. At load time, `captureIndex` resolves the index once.
  3. For each match, the entity's `startLine`/`endLine` come from the node of that capture.
- **Alternative Flows**: without `span_capture`, the entity keeps the usual span — the start
  of the name node to the end of its parent.
- **Error Scenarios**: nonexistent capture → -1 → previous behaviour.
- **Postconditions**: the entity's `line_number`/`end_line` describe the whole construct,
  which also improves `ast_source --entity` for it.
- **Affected Files**: `internal/ast/query_loader.go`, `internal/ast/treesitter_adapter.go`.

## Test Cases & Acceptance Criteria

### Feature: attribution of an embedded block to the host
Ref: UC-01

#### Scenario: the sibling above is not the host
```gherkin
Given an XML whose <entry> has <key> on line 4 and <value> opening on line 5
  And the <value> carries an INSERT on line 7 and closes on line 9
  And an entity query with span_capture covers the element that wraps the <entry>
When the file is parsed
Then the INSERTS reference has as its origin the unit that wraps the block
  And does not have as its origin the Element of the <key>
```

#### Scenario: an entity that ends inside the block does not host
```gherkin
Given an entity whose span is only the line on which the block starts
  And an entity whose span contains the whole block
When the attribution picks the host
Then it picks the one that contains the whole block
```

#### Scenario: a block with its own unit keeps its origin
```gherkin
Given an embedded block that declares a procedure
When the attribution to the host runs
Then the origin remains the procedure declared in the block
```

#### Scenario: no entity contains the block
```gherkin
Given an XML with no entity that contains the whole block
When the file is parsed
Then the origin of the reference is the file
  And no edge is discarded
```

### Feature: span_capture
Ref: UC-02

#### Scenario: the entity receives the span of the captured node
```gherkin
Given an entity query that captures the whole element as @scope
  And declares span_capture: scope
When the file is parsed
Then the entity's line_number is the first line of the element
  And end_line is the last line of the element
```

#### Scenario: without span_capture nothing changes
```gherkin
Given the same query without span_capture
When the file is parsed
Then the entity's span goes from the name node to the end of its parent
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/treesitter_embedded.go` | Modified | real interval of the block; containment in `hostEntityAt` |
| `internal/ast/query_loader.go` | Modified | `SpanCapture` + `SpanIdx` |
| `internal/ast/treesitter_adapter.go` | Modified | the entity's span comes from `span_capture` when declared |
| `internal/ast/query_loader.go` | Modified | `HostLabels` on `EmbeddedBlock`, `NameIsData` on the query |
| `internal/ast/rebuild_index.go` | Modified | `callerSet` without a fixed list of labels |
| `internal/ast/shard_cache.go` | Modified | `shardCacheVersion` 7 → 8: the origin of every embedded-block edge changes, and the cache is keyed by content hash |
| `internal/ast/embedded_lang_resolution_test.go` | Modified | signature of `attributeToHostEntity` |
| `internal/ast/embedded_host_span_test.go` | Created | case through the real parse, containment and `span_capture` |
| `docs/specs/ast_module.md` | Modified | new field documented |
| `docs/specs/embedded_language_parsing.md` | Modified | section 3b: attribution to the host, strict containment and the off-by-one error |
| `docs/tasks/embedded-block-carries-its-own-language.md` | Modified | correction note about the defective version described there |

Outside this repository: the consumer project's `ast.queries_dir` (project grammar,
`merge: true`).

## Trade-offs & Decisions

- **`span_capture` governs only the line interval**, not `entitySource` nor the
  complexity. The export verdict reads the text of the DECLARATION (the tag, in the case of XML) and the
  complexity is scored on the subtree of the declaration itself; extending the scope of both
  together would be a behaviour change in every grammar that started using the field, with
  nothing asking for it.
- **Containment of the whole block, not of the first line.** More restrictive: a host that
  ends in the middle of the block stops being picked. It is what eliminates the wrapper without
  needing a list of labels.
- **No `host_labels`.** It would be the second mechanism for the same end, and the containment
  rule already solves it. If one day two legitimate units nest and the innermost
  one is not the desired one, then there is a question — today there is none.

## Technical Debt

- [ ] The entity uid is `(path, name)`, so two units with the same name in the same file
  collapse into one node, with the line of the first. It shows up now that units become hosts
  of DML edges: the edges of both leave the same node. The real fix is to qualify the uid
  (position, or the path to the root), which changes the entity uid in every grammar — outside
  the scope of this work.
- [ ] `hostEntityAt` is O(entities of the file) per embedded block. In an XML of 400 thousand
  lines with dozens of blocks, that is dozens of sweeps over a few thousand
  entities — measurable, not dominant. If it hurts, the way is to index the entities by
  interval once per file.

## System Knowledge

- **The `endLine` of a tree-sitter entity is the end of the PARENT of the captured node**
  (`treesitter_adapter.go`, in the capture loop). That is why every XML `Element` entity
  has `end_line == line_number`: the parent of `(STag (Name) @name)` is the start tag. It holds for
  any data grammar that captures the name inside the tag.
- **`innerOffset` is an offset, not a line.** It is `primeiraLinhaDoBloco - 1`, because the
  sub-parse reports 1-based lines inside the block. Confusing the two is the error this
  work fixes, and it does not show up in a unit test of `attributeToHostEntity`: the
  function always received an absolute line in the tests.
- **A test about `pf.References` can pass with the whole defect in place** — already
  recorded in memory, confirmed once again here: the case that was missing was the CALLER's,
  through the real parse path.

## Progress Log

### 2026-08-19
- Diagnosis closed on the consumer's live graph (not deduced): the edge leaves an
  `Element` named `key`, whose node is at the first occurrence of the tag in the file (uid
  `(path, name)`), and **no** node in the file satisfies `line_number <= L <= end_line` for
  a line in the middle of the block — which proves both defects at once.
- Confirmed that `merge: true` already exists, so the premise "the project grammar has to
  be an entire copy" that was circulating in the report is out of date; the consumer's grammar
  is already partial.
- Log opened with objective, design and plan.
- T1, T2, T3 implemented, plus `shardCacheVersion` 7 → 8 — without the bump, a corpus with
  unchanged files would keep serving the old origin from the shard cache,
  silently. It is the same reason as the 4 → 5 bump.
- T4 closed with the proof that was missing in the previous round: `TestEmbeddedBlockIsAttributedToTheContainingUnitNotTheSiblingAbove`
  runs the REAL parse and, with the caller reverted to `innerOffset`, fails with
  `SourceName = "key"` — exactly the symptom reported. `internal/ast` green in 77 s
  (`-tags fts5`).
- T5 closed on the consumer side: its project grammar (which was already partial, with
  `merge: true`) gained the query for the named unit with `span_capture`, and the usage instructions
  — what the customization produces, why the node is named the way it is, and the ready-made queries —
  went into ITS AGENTS.md, not here: it is domain vocabulary.
- Measurement that changed the design midway: with simple containment only, 3 of the 28
  references of the flow still left the `<value>` wrapper (one-line statements). Hence
  STRICT containment and the test `TestHostMustExtendBeyondTheBlockNotCoincideWithIt`.
- T6 closed: `ast_module.md` (field + example + table), `embedded_language_parsing.md`
  (new section 3b), a correction note in the task log of 2026-08-15 — which described the defective
  version and would go on lying silently — and three new memories plus the update of the
  memory that still recorded this as an open pendency.
- Corpus probe removed from the repository (it was `internal/ast/zz_probe_corpus_test.go`); the
  measurements it produced are in this log.
- Missing: verification on the consumer's live graph (reindex in progress — the corpus is large) and
  the full `internal/ast` suite once the machine is free.
