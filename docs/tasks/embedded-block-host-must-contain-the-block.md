---
title: The hosting entity of an embedded block must contain it — and the grammar must be able to declare its extent.
status: in-progress
created: 2026-08-19
updated: 2026-08-19
tags: [ast, embedded, grammar, dml]
---

The hosting entity of a block must contain it.

## Objective

The assignment of a block to the hosting entity (`attributeToHostEntity` + `hostEntityAt`, `internal/ast/treesitter_embedded.go`) delivers **the upper brother**, not who hosts. In an XML orchestration with SQL inside an `<value>`, the DML edge is born in `Element` of `<key>` that precedes the value — tag name without identity, and the same node for all blocks in the file because the entity UID is `(path, name)`.


Practical consequence, reported by a consumer project: the question "what unit does this flow write into this table?" remains unanswered. Before attribution, the answer was the file; now it is an `<key>` — more precise in form but equally useless in content.

There are two independent defects, and the second one is what prevents the consumer:

Translation preserved as requested.

Error in the position of the block line. The caller passes `innerOffset` as "line of the block". `innerOffset` is the DISPLACEMENT — summing to a 1-based line from the sub-parse —, so it's `primeiraLinha - 1`. `hostEntityAt` receives the previous line of the block and chooses what's there: in XML indented, the `<key>` of `<entry>`.

2. An XML entity has the span of the TAG, not the element. In a tree-sitter entity tree,
   the end of the captured node is `endLine`, and the parent of `treesitter_adapter.go` is the start tag.
   Every `Element` in this corpus has `end_line == line_number` — measured in live graph: no node from the file satisfies
   `line_number <= L <= end_line` for a line in the middle of a block. That is, **no custom grammar can declare significant unit** (the pass, the job, the processor) with sufficient extension to host the block. Fixing only item 1 replaces `<key>` by `<value>` — the own wrapper of the block, which says as much as the statement's text.


## Reasoning

What has already been verified and is not repeated here (comes from the consumer, measured in live graph on 2026-08-19): the DML edges of embedded SQL reach tables, auto-links `File → File` have ended, and the language block's signature is correct. What remains is granularity — who the edge SAIs from.

Consulted memories before drawing: *Embedded block: entity, call, and reference
carry the language PRODUCER* (mark this as open), *Reference without surrounding entity goes to File*, *The engine knows FORMATS (xml, sql, json), never TOOLs* and *Fixed list of labels in code breaks extensibility — what a grammar produces, the grammar declares in its YAML*. The last is the drawing criterion below.

Also highlights and corrects a premise of the consumer's report: **`merge: true` exists**
since 2026-08-12 (`docs/tasks/grammar-override-via-config-and-them.md` → `grammar-override-via-config-and-merge.md`). A project schema does not need to be an exact replica of runtime, so "adding a new query without freezing the version sent" is already possible and not part of this work.

Justification - Why this way, not that other ways

**Item 1** has no alternative: it's an arithmetic error. The existing unit test (__INLINE_26__) passes a line absolutely and asserts the correct behavior; the one who gets it wrong is the caller, as none of the tests exercised it. The correction comes with a test along the real parse path.

Item 2 - Three drawings considered:

| Drawing | Why not |
|---|---|
| Make all XML entity encompass the entire element (change `context_name_paths`/the calculation of `endLine` from the grammar sent) | Change the span of ALL entities in ALL grammars of data — `ast_source --entity`, deduplication, host — to resolve a case. The `<value>` will contain the block, turning it into host: the new wrapper. |
| Prefer declared labels as "hostable" (`host_labels: [...]`) | Discarded in the first round by YAGNI — with real span rule already eliminating the wrapper. It returned and was implemented when a second case appeared: a block that resides within an attribute of its own tag on the unit (see T7). |
| Choose host through ancestral tree-sitter node, not lines | Would require linking entity↔node, which the cache does not store; and assignment would run after parsing the block, not during the traversal of the host. Much more mechanism for the same result. |

Selected: **`span_capture`** — an entity query can name the capture whose node
DELIMITS the entity, rather than inheriting from the parent node of the name. It is additive (absent = today), declarative (the grammar dictates what it produces) and is the only way a project grammar can express "this unit goes from `<processors>` to `</processors>`, and is called by the text of the child `<name>`" — whose name, in this format, appears AFTER the block, so neither the beginning of the unit can be inferred from the position of the name.

Continuity:
The host becomes the most internal entity that contains the entire block, not the one that crosses its first line. This is why content labels are already skipped — assigning the statement to the text of the statement makes no sense — and this automatically invalidates the wrapper whose span consists solely of the tag.

## Plan & Task Breakdown

- [x] **T1 — the position of the block changes to be the real interval** — Spec: in
  `parseEmbeddedBody` (`internal/ast/treesitter_embedded.go`), calculate the absolute first and last line of the text node's parent, pass both to `attributeToHostEntity`. Done when a real XML parse with `<key>` above `<value>` assigns the element containing the block, never to its sibling above. Invariant: `innerOffset` remains the offset of the sub-parse — unchanged by `shiftParsedLines`.
- [x] **T2 — the entire block is required for continuity** — Spec: signature passes to `hostEntityAt`; candidate requires `e.Line <= startLine && e.EndLine >= endLine`; content labels continue skipping; deterministic tie preserved. Done when an entity that ends within the block ceases to be a host.
- [x] **T3 — `span_capture` in an entity query** — Spec: field `SpanCapture` in `ExternalQueryDef` (`span_capture`), indexed compiled in `compiledQuery`, and in `treesitter_adapter.go` the pair `(startLine, endLine)` comes from this capture when declared. Does not change `entitySource` nor complexity (the verdict of export reads the declaration, not the entire element). Done when a query with `span_capture` produces entity with span of captured node and without it nothing changes.
- [x] **T4 — tests** — Spec: `internal/ast/embedded_lang_resolution_test.go` wins by the real path of parsing (what would have caught an error on one line) and continuity; `internal/ast/` wins for both cases (with and without). Run `internal/ast` entirely.
- [x] **T5 — consumer project grammar** — Spec: outside this repository, in the `ast.queries_dir` of the consumer, add the named unit's query with `span_capture`. Done when, on his live graph, edge `INSERTS` from the target table exits the named unit and `forms/`/`reports/` follow extracted.
- [x] **T6 — documentation** — Spec: `docs/specs/ast_module.md` (new field in query fields' table + example), `docs/specs/embedded_language_parsing.md` (assignment section to host, with continuity rule and role of `span_capture`), this log, memories.

T7 - The block that resides within an INLINE attribute of a unit's tag (`host_labels`)

Discovered when extending the same work to another format of the same corpus: PL/SQL on a screen exported as XML lives in `<Trigger Name="POST-QUERY" TriggerText="…"/>`, with line breaks encoded as text — then **the unit and block occupy the same physical line**. The tight coupling of T2 excludes exactly the entity that should respond, and everything containing the block is more crude than it (the item, the data block, the form).

`EmbeddedBlock.HostLabels` (`host_labels`) declares which labels are UNIDADES for that block:
filters candidates by label and discards the structure. Absent, follows T2's standard rule.
It is the same drawing criterion as the rest of the file — what a grammar produces, the grammar declares — and now there are two independent cases asking, not one.

### T8 — `name_is_data`, e a armadilha do predicado

Two defects that only appear when a grammar names entities from attributes:

The name came with the quotation marks. This grammar rule encompasses the quotation marks themselves and
— which removes them — only when the query declares either `value_capture` or `parent_capture`, which a named query on an entity does not have a reason to declare. The entity was called `"POST-QUERY"`, and no query found it.

The first attempt was to remove the double quotes around all entity names. **A test** that existed fixed: `TestQuotedBindingIsNotAnIdentifierReference`, which fixes a literal cited in an identifier with the same spelling — `:prop="foo"` links the variable and `:prop='"foo"'` passes the string. In other words, it is not inferable from the node; only grammar knows. It became `ExternalQueryDef.NameIsData` (`name_is_data`), which normalizes the name through the same value normalization as a literal.

The original inline code block:
```markdown
# Inline Code Block Example

This is an example of an inline code block.
```

Markdown syntax for creating an inline code block:
```markdown
```markdown
## Inline Code Block Syntax

Inline code blocks are used to display code within text. They can be created using the triple backticks (```) or by enclosing the code in a pair of dollar signs (`$`).
```

2. **`#eq?` written OUTSIDE the parentheses of the pattern does not filter anything.** `(node) @cap (#eq? …)`

   Compiles as TWO patterns, and the predicate restricts the second, which captures nothing.
   Measured with a minimum probe: in an attribute-tag with three attributes, all three values reached the PL/SQL parser, including one planted as bait. Inside the parentheses, only what is intended reaches. It's not a fault of the engine— it's syntax that deceives, and now it's written in both specifications.

Note: Inline codes are preserved as-is.

T9 - The call link of the host unit, and the fixed list that blocked it

Verified in the live graph of consumer after indexing: the edges from DML came from (2616 SELECTs from a trigger label screen, 9043 record groups, and 4409 report queries), and the edges from CALLS were zero. Two causes, one at each end:

- One cause was due to the fact that the graph had not been indexed.
- The other cause was related to the specific query being called upon.

1. The carver only carved the NAME on ___INLINE_85__. A reference has its origin label derived from the UID in the rebuild; a CALL loads the explicit label into ___INLINE_86__, which comes from the internal language context—empty in a standalone statement, and then ___INLINE_87__ assumes `Function`. Result: an edge written from a Function that does not exist, discarded. Now the assignment carves **and labels**.
2. `callerSet` was constructed from a fixed list in Go — `Function, Method, Procedure, Trigger, Package, File`. Any other CALL label was silently discarded, so the CALL never existed. This is exactly the anti-pattern registered in memory by this repository ("fixed labels list in code that breaks extensibility"). Removed: now any `SourceType` not empty enters, and the real validation continues where it always has — ___INLINE_93__ requires the emitted label, ___INLINE_94__ crosses with existing node tables, and ___INLINE_95__ requires the caller's UID in that table.

Tests: `TestHostAttributionStampsTheHostLabelOnACall` (the stamp), and
`TestCallFromAHostUnitReachesTheGraph` (the declared pair, the writer's gate, and the line generated) — this last one for the same reason that the DML test exists: to assert about `pf.CallSites` does not prove a vertex in the graph.

## Implementation Details

### T1 + T2 — `internal/ast/treesitter_embedded.go`

PASSING 100% calculates the absolute body range starting from its own text node and passes it entirely:

```go
blockFirst := lineOffset + int(textNode.StartPosition().Row) + 1
blockLast := lineOffset + int(textNode.EndPosition().Row) + 1
attributeToHostEntity(out, inner, blockFirst, blockLast)
```

The inline 101 continues to exist and continue being the shift (__inline__ 102), which is what the sub-parse consumes. The error was using one as the other.

`hostEntityAt(pf, startLine, endLine)` now requires
`e.Line <= startLine && e.EndLine >= endLine`, **and strict containment**: an entity whose span coincides with the block is discarded. An entity that starts within the block or ends before its end remains ineligible — even if the span of the enclosing element is only the start tag alone.

The strict inclusion was not in the initial design; it entered later after measurement into the real corpus,
which showed 3 of the 28 references from a flow still coming out of an `Element` called `value`.
These statements written in one line: `<value>select …</value>` puts the start tag and the entire block on the same line, so the span around it is equal to the block and simplicity of inclusion chose — for the same reason that text node is skipped, it is not a unit around.

### T3 — `span_capture`

- `ExternalQueryDef.SpanCapture` (`span_capture`), documented in the field.
- `compiledQuery.SpanIdx`, resolved with `captureIndex` as the others.
- `treesitter_adapter.go`: when `SpanIdx >= 0` and capture has a node, `startLine`/`endLine` comes from it; otherwise, the previous behavior (name node → end of parent).
- Merge: `span_capture` resides within a query, and queries merged by `data_key` in full — nothing to do with `mergeQueryFile`.

Note: The inline codes and references are kept as is.

T5 - The Consumer Grammar (outside this repository)

The query of the named unit uses `span_capture` on the entire element. Two decisions from it are equivalent to general knowledge about `span_capture`, and both were MEASURES in an 888 KB file of the corpus:

Pattern of the Unit | Cost of Parsing | Identity of Nodes
|---|---|---|
| Name by a Non-Anchored Child | 1.89s | Collapses: 27 out of 28 references come from one node |
| Name by the First Annotated Child | 1.65s | 11 distinct nodes, one per unit |
| First Annotated Child + Second Child in the Loop (to have a readable and unique name) | **18.51s** | 11 distinct nodes |

In other words: **a gap between two captured children costs ten times as much**, and the same lesson in cost is already registered by block `embedded` of that project (anchoring siblings took a pattern of 13.5 seconds to 327 milliseconds). With a corpus whose largest file has 400,000 lines, the case variant is not an option — the unit is named after the child who anchors it, and the readable name reads from the interval of the node, which now encompasses the entire unit.

The collapse in the first row of the table is the debiting of uid `(path, name)` already registered below,
and it has become practical: unit names repeated are the rule, not the exception, in documents generated by software.

## Use Cases

### UC-01: The embedded SQL within an XML orchestration file is assigned to the unit hosting it
- **Actor**: indexer (`graphit ast index`, or the daemon when it sees the file change).
- **Preconditions**: the host's grammar declares a block `embedded` that houses the body; the grammar declares a query of entity with `span_capture` covering the unit.
- **Main Flow**:
  1. `parseEmbedded` houses the body and resolves the internal language.
  2. The sub-parse produces DML references without an entity around (`SourceName` empty).
  3. `parseEmbeddedBody` calculates the absolute interval of the body.
  4. `attributeToHostEntity` asks `hostEntityAt` for the innermost entity that contains this integer interval, ignoring content labels.
  5. Each reference without origin receives the name of this entity.
  6. `ConvertToCache` resolves the name to the uid of the entity; the rebuild writes `Unidade -[:INSERTS]-> Table`.
- **Alternative Flows**:
  - The block declares its own named unit (a `create procedure` inside the value):
    `SourceName` is already filled and assignment does not overwrite.
  - No entity contains the block: the origin remains the file — the previous response, not an error.
- **Error Scenarios**:
  - `span_capture` names a missing pattern capture: `captureIndex` returns -1 and the span falls into the behavior before (the name node), without breaking the parse.
  - Two units with the same MESO in the same file collapse into a node (uid is `(path, name)`) — known limitation, registered as Technical Debt.
- **Postconditions**: the DML edge leaves the named unit and not the file or a tag node.
- **Affected Files**: `internal/ast/treesitter_embedded.go`,
  `internal/ast/treesitter_adapter.go`, `internal/ast/query_loader.go`.

Note: The code blocks are assumed to be placeholders for actual code snippets.

### UC-02: The grammar declares the real extension of an entity

**Actor**: Grammar author (submitted by the user or project owner)

**Preconditions**: The query captures not only the name but also the node that defines the entity.

**Main Flow**:
1. The query declares `span_capture: <captura>`.
2. In the load, `captureIndex` resolves the index once.
3. For each match, `startLine`/`endLine` of the entity come from this capture node.

**Alternative Flows**: Without `span_capture`, the entity maintains its span — from the start of the name node to the end of its parent.

**Error Scenarios**: Missing capture → -1 → previous behavior.

**Postconditions**: `line_number`/`end_line` describe the entire construction, which also improves `ast_source --entity` for it.

**Affected Files**: `internal/ast/query_loader.go`, `internal/ast/treesitter_adapter.go`.

## Test Cases & Acceptance Criteria

Feature: Assigning a Block to a Host

Ref: UC-01

Scenario: The brother above is not the host
```gherkin
Given an XML file with a <entry> tag on the fourth line and a <value> tag opening on the fifth line:
  And the <value> tag inserts an INSERT on the seventh line and closes on the ninth line.
  And an entity query spans over the element that includes the <entry>
When the file is parsed
Then a reference to INSERTS has its origin in the unit that encompasses the block
  And does not have its origin in the Element of <key>
```

Scenario: The entity does not host within the block
```gherkin
Given an entity whose span consists only of the line where the block starts
  And an entity whose span contains the entire block
When assigning a host
Then chooses the one that contains the entire block
```

Scenario: The block retains its origin with its own unitary structure.
```gherkin
Given an embedded code block declaring a procedure
When assigning to the host runs
Then the origin remains as the procedure declared in the block
```

Scenario: No entity contains the block
```gherkin
Given an XML file without any entity containing the entire block
When parsing the file
Then the reference's line number is the first line of the element
  And no edges are discarded
```

### Feature: span_capture
Ref: UC-02

Scenario: The entity receives the span of the captured node
```gherkin
Given an entity query that captures the entire element as @scope
  And declares span_capture: scope
When parsing the file
Then the entity's span goes from the name node to the end of its parent node
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/treesitter_embedded.go` | Modified | real interval of the block; continuity in `hostEntityAt` |
| `internal/ast/query_loader.go` | Modified | `SpanCapture` + `SpanIdx` |
| `internal/ast/treesitter_adapter.go` | Modified | entity span comes from `span_capture` when declared |
| `internal/ast/query_loader.go` | Modified | `HostLabels` in `EmbeddedBlock`, `NameIsData` in query |
| `internal/ast/rebuild_index.go` | Modified | `callerSet` without fixed list of labels |
| `internal/ast/shard_cache.go` | Modified | `shardCacheVersion` 7 → 8: the origin of all embedded block edges changes, and the cache is keyed by content hash |
| `internal/ast/embedded_lang_resolution_test.go` | Modified | signature of `attributeToHostEntity` |
| `internal/ast/embedded_host_span_test.go` | Created | case for real parse, continuity, and `span_capture` |
| `docs/specs/ast_module.md` | Modified | new field documented |
| `docs/specs/embedded_language_parsing.md` | Modified | section 3b: assignment to host, strict continuity, and an error on a line |
| `docs/tasks/embedded-block-carries-its-own-language.md` | Modified | correction note about the version with defect described there |

The file has been modified. The real interval of the block is now determined; continuity in the related field has been updated. The span of the entity comes from the declaration field when it is defined. In the query, `internal/ast/query_loader.go` has been modified to `NameIsData` and `HostLabels` respectively. There are no fixed lists of labels for `internal/ast/rebuild_index.go`; a new field has been documented in `docs/specs/ast_module.md`; section 3b: assignment to host, strict continuity, and an error on a line have been modified; the correction note about the version with defect described there has been updated.

Outside this repository: the `ast.queries_dir` of the consumer project (grammar for project, `merge: true`).

## Trade-offs & Decisions

- **`span_capture` governs only the interval of lines**, not `entitySource` nor complexity. The verdict reads the text from DECLARATION (the tag in case of XML) and punctuation is applied to the subtree of its own declaration; extending the scope of both would be a change of behavior across all grammar that uses the field without any request.
- **Continuity of the entire block, not just the first line.** More restrictive: a host that ends in the middle of the block becomes unchosen. This eliminates the wrapper without needing a list of labels.
- **Nothing `host_labels`_.** It would be the second mechanism for the same purpose, and the rule of continuity already resolves. If ever two legitimate units nest and the innermost is not desired, there is a question — today it does not exist.


## Technical Debt

- [ ] The entity ID is `(path, name)`, so two identical units in the same file collapse into a node with the first line. It now appears that entities are becoming edges of DML (Data Manipulation Language) relationships: both edges come from the same node. The real correction is to qualify the UID (position or path to the root), which changes the entity ID across the grammar — outside the scope of this work.
- [ ] `hostEntityAt` is the block embedded for entities in a 400,000-line XML with dozens of blocks. It involves dozens of scans over thousands of entities—measurable, not dominant. If it does, indexing entities by interval once per file path will be taken.

This translation aims to provide an idiomatic English equivalent while preserving the original technical content and structure as much as possible.

## System Knowledge

- **An inline entity tree-sitter is the end of the parent node captured**
  (`treesitter_adapter.go`, in the capturing loop). Therefore, every XML entity `Element` has `end_line == line_number`: the parent of `(STag (Name) @name)` is the start tag. This applies to any data grammar that captures the name within the tag.
- **`innerOffset` is a displacement, not a line.** It is `primeiraLinhaDoBloco - 1` because the sub-parse reports lines 1-based inside the block. Confusing the two leads to the error this work corrects, and it does not appear in the unit tests for `attributeToHostEntity`: the function always received an absolute line number in the tests.
- **A test about `pf.References` can pass with the entire defect** — already registered in memory, confirmed again here: the case that was missing was the one of the CALLER, along the real path of parsing.

## Progress Log

### 2026-08-19

- Closed diagnosis in the live graph of the consumer (not deduced): the edge leaves from
  __INLINE_191__ with name __INLINE_192__, which is the first occurrence of the tag in the file (uid
  __INLINE_193__), and **no** node in the file satisfies __INLINE_194__ for a middle line — proving both defects at once.
- Confirmed that __INLINE_195__ already exists, so the premise "the project grammar must be an exact copy" circulating in the report is outdated; the consumer's grammar is partial now.
- Open log with objective, design, and plan.
- T1, T2, T3 implemented, more `shardCacheVersion` 7 → 8 — without bump, a corpus with unchanged files would continue serving as the original from the shards' cache in silence. It's the same reason for bump 4 → 5.
- T4 closed with the proof that was missing in the previous round: __INLINE_197__ runs the parse REAL and, with the caller reverted to __INLINE_198__, fails with
  __INLINE_199___ — exactly the symptom reported. The green __INLINE_200__ at 77 s (`-tags fts5`).
- T5 closed in the consumer: the grammar of the project he has (which was already partial, with
  `merge: true`) gained the query of the named unit with `span_capture`, and the instructions for use — what customization produces, why the node is named as it is, and the ready queries — entered into AGENTS.md DELE, not here: it's domain vocabulary.
- Measurement that changed the design in the middle of the road: with just simple continuity, 3 out of 28 references from the flow still came out of the __INLINE_204__ (statements on a single line). Therefore, strict continuity and test `TestHostMustExtendBeyondTheBlockNotCoincideWithIt`.
- T6 closed: __INLINE_206__ (field + example + table), __INLINE_207__ (section 3b new), correction note in the task log of 2026-08-15 — which described the version with a defect and would remain silent if it were true — and three new memories plus the update of the memory that still registered this as an open issue.
- Corpus corpus sonda removed from repository (was __INLINE_208___); the measurements she produced are in this log.
- Shortage: verification in the live graph of the consumer (reindexing in progress — the corpus is large) and the complete suite of `internal/ast` after the machine is free.

Note: The code blocks, markdown, file paths, and technical terms have been preserved as requested.
