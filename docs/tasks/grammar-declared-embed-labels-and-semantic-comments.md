---
title: Grammars declare which labels get a vector (embed_labels), and comments become semantically searchable
status: done
created: 2026-08-16
updated: 2026-08-16
tags: [ast, grammar, embedding, semantic-search, extensibility]
---

# Grammars declare which labels get a vector (`embed_labels`), and comments become semantically searchable

## Objective

Two things, and the second replaced the first mid-task.

**The starting request:** comments captured by the grammars should support both FTS
and semantic search, and therefore should be in the SQLite index that serves both.

**The measured answer to that:** FTS already worked and semantic did not — see
[Measurements](#measurements). Comment text is the `Comment` entity's `name`, so it
reaches `entity_fts.name_split` like every other entity. It never reached
`entity_vec`, because `internal/ast/embedder.go` filtered the embedding scan through
a package-level `var embeddableLabels = []string{...}` of sixteen labels that did not
include `Comment`.

The correction that redefined the task was rejected because adding `Comment` to that slice was not allowed. "It makes sense to have a fixed list of nodes that will perform embedding — everything needs to be customizable by grammar file, it cannot be fixed in the code, this breaks extensibility. Each grammar must define its own node in its yml."

The list itself was the defect, and the comment gap was one symptom of it. A grammar
the binary has never seen — installed from the Hub as a `language` artifact, or
written into `ast.queries_dir` — produces labels that are absent from any list
compiled into Go, so **nothing that grammar indexes is ever embedded, and nothing
reports it**. So the deliverable became: move the decision into the query YAML, where
every other language-shaped decision already lives.

## Implementation Details

### 1. `embed_labels`, a new language-level field

`ExternalQueryFile.EmbedLabels` (`internal/ast/query_loader.go`) — `embed_labels` in
the YAML — names the graph labels of that language that get a vector. It joins
`comment_types`, `context_types`, `declaration_types`, `self_keywords`,
`anon_func_types`, `target_rules` and `exports` as a per-language declaration.

Wired into the three places a language-level field has to be wired into:

- `mergeQueryFile` — declared replaces, omitted inherits, the same rule as every
  other list field.
- `hasLangConfig` — so a file declaring only `embed_labels` still counts as a
  language config file.
- `invalidateDerivedQueryCaches` — the new `embedLabelCache` is dropped with the rest
  when a grammar is installed or edited.

### 2. `EmbedLabelsForLang` — resolution by language alone

`EmbedLabelsForLang(projectDir, lang)` walks project → user → runtime and stops at
**the first level whose file for that language declares `embed_labels`** — not at the
first level that mentions the language. That distinction matters: a project file
overriding one pattern of a language, and silent about embedding, must not turn
embedding off for it.

It matches on language with **no extension**, unlike `resolveQueriesForLang`. The
caller is the embedder, which reads entities out of the parse cache, where an entity
carries the language that produced it and never the extension of the file it came
from. That is also the correct answer for embedded blocks: the `<style>` of a `.vue`
file carries `css`, so `css.yaml` answers for it, which an extension-keyed lookup
would get wrong.

### 3. The embedder resolves per `(lang, label)`

`embeddableLabels` is gone. `embedRanker` (`internal/ast/embedder.go`) memoises each
language's declared list the first time an entity of that language is seen, and
answers two things at once: *is this label embeddable*, and *at what rank*.

Rank is the label's index in its own language's list, carried on `entityRow.Rank`.
It replaces the position in the old global slice as the tie-break for the dedup: one
`(path, uid)` can carry two labels (a TypeScript `class Foo` beside `interface Foo`,
a Table beside a same-named View) and the embedding cache is keyed on `(relPath, UID)`
**without** the label, so the two collide on one entry. Comparing ranks across
languages never happens — a collision is one entity in one file, so both sides come
from the same grammar.

`labelOrder(buckets)` sorts the buckets by the best rank any of a label's rows
carries, then by name. `RunCycle`'s batch loop and `scanPending`'s dedup both walk
it, and they must walk the same order: the dedup decides which label keeps an entity
by being there first.

### 4. `EmbeddingConfig.ProjectDir`, split from `RepoRoot`

The ranker needs the project whose grammars answer. `RepoRoot` was the obvious
candidate and is the wrong one: it is documented as *a place to read source from*,
and blanking it is a supported state meaning "do not read the tree"
(`ast.index_source: false`, and `TestEmbeddingWithoutRepoRootStillEmbedsTheEntity`
asserts it). Blanking "which labels does this language embed" is a different thing —
it embeds nothing — and one field cannot mean both. Both callers
(`cmd/graphit/commands/ast.go`, `RunEmbeddingLoop`) set them to the same directory.

### 5. All 45 shipped grammars declare their own list

Curated per grammar rather than copied: the labels whose **name or body carries
meaning someone would search for in words**. Excluded across the board are
`Parameter`, `Value`, `AttributeValue`, `Pair` (except in the data formats where the
key IS the content), and the import/include/export statement labels.

This is where the old fixed list turns out to have been silently ignoring most of the
corpus. It knew sixteen labels; the grammars produce far more, and every one outside
those sixteen was keyword-only with nothing saying so — `clojure`'s `Macro`,
`Protocol`, `Record`, `Namespace`; `graphql`'s `ObjectType`, `Query`, `Mutation`,
`Fragment`; `protobuf`'s `Message`, `Service`, `RPC`, `Oneof`; `hcl`'s `Resource`,
`Provider`, `Output`; `dockerfile`'s `Stage`, `Instruction`; `cobol85`'s `Program`,
`Paragraph`, `Section`; `css`'s `CssClass`, `CssVariable`, `Keyframes`; the SQL
family's `Column`, `Constraint`, `Cursor`, `Exception`, `MaterializedView`,
`Synonym`, `StoredProcedure`.

### 6. Comments: embedded, and embedded once

`Comment` is declared by every shipped grammar that declares `comment_types`
(`plpgsql.yaml` is the single exception — it declares none).

`scanPending` skips the source snippet for `Comment` alone. Every other label's
snippet is a body its name does not carry; a comment's snippet is the comment
itself, marker syntax included, so slicing it appends the name a second time and
pays a shard read to do it.

## Measurements

Taken on this repository before the change, and they are what turned "make comments
searchable" into "make comments *semantically* searchable":

| Question | Method | Result |
|---|---|---|
| Are comments in FTS? | `ast_search mode:"fts"` for a phrase that exists only inside a comment | **Yes** — two `type=comment` hits, `internal/ast/fts_sqlite.go:264` and `internal/wiki/fts.go:408` |
| Are comments in the vector index? | `ast_search mode:"semantic"` for the same phrase | **No** — zero comment rows; only `Function` results, top score 0.39 |
| What is actually embedded? | `SELECT entity_type, count(*) FROM entity_vec_map GROUP BY 1` on the live index | 8695 vectors: Function 4460, Variable 2301, Method 871, Struct 478, Constant 450, Interface 83, Type 44, Parameter 6, Class 2. **No `Comment` row** |
| How much was invisible? | `graphit_ast_schema` live node counts | ~15,100 `Comment` nodes across go/yaml/tsx/ts/css/bash/js/xml — **more than the entire vector index held** |

Where the text lives, confirmed by reading the DDL in `internal/ast/fts_sqlite.go`:

- `entity_fts(uid, name UNINDEXED, name_split, docstring, entity_type, path, line_number)` —
  `name_split` carries weight 10.0, and a comment's `name` is its text, which is why
  keyword search over comments already worked.
- `entity_vec(embedding float[768])` + `entity_vec_map(uid, vec_rowid, name, docstring, …)`.
- The `value` property is in **neither**. Nothing in this task changes that — comment
  text never lived in `value` in the first place; both adapters
  (`extractCommentsAntlr`, and the tree-sitter comment pass) build a `Comment`
  entity whose `Name` is the cleaned text.

## Use Cases

### UC-01: A grammar declares which of its labels are semantically searchable
- **Actor**: whoever authors or installs a grammar — a shipped YAML, a file in
  `ast.queries_dir`, or a Hub `language` artifact.
- **Preconditions**: the query YAML declares `language`, and queries or other
  language-level fields.
- **Main Flow**:
  1. The author adds `embed_labels:` with one label per line, most important first.
  2. `parseQueryFile` loads it into `ExternalQueryFile.EmbedLabels`.
  3. `EmbedLabelsForLang(projectDir, lang)` resolves it — project, then user, then
     runtime — caching per `(projectDir, lang)` in `embedLabelCache`.
  4. `scanPending` calls `embedRanker.rank(ent.Lang, ent.Label)` for every cached
     entity and keeps only the ones their own language named.
  5. `RunCycle` embeds the kept rows and `RebuildFromCache` writes the vectors into
     `entity_vec` / `entity_vec_map`.
- **Alternative Flows**:
  - A project file with `merge: true` and no `embed_labels` inherits the level below
    it, so overriding one pattern does not silence embedding for the language.
  - A project file that *does* declare `embed_labels` replaces the lower level's list
    outright — the only way to shorten one.
- **Error Scenarios**:
  - Label the grammar cannot emit: matches nothing, at no cost.
    `TestEmbedLabelsAreLabelsTheGrammarProduces` fails for a shipped grammar.
  - Field omitted: that language embeds nothing.
    `TestEveryShippedGrammarDeclaresEmbedLabels` fails for a shipped grammar; a
    third-party grammar gets no error, which is the accepted cost of a field that has
    to be optional to stay backward-compatible (see Technical Debt).
- **Postconditions**: `ast_search mode:"semantic"` returns entities of the declared
  labels for that language; keyword search is unchanged, because `entity_fts` never
  consulted this list.
- **Affected Files**: `internal/ast/query_loader.go`, `internal/ast/embedder.go`,
  `internal/ast/queries/*.yaml`.

### UC-02: An agent finds a comment by meaning rather than by keyword
- **Actor**: an agent (or a user) running `graphit_ast_search`.
- **Preconditions**: the file's grammar declares `Comment` in `embed_labels`; the
  embedding cycle has run.
- **Main Flow**:
  1. `scanPending` sees a `Comment` entity whose language ranks it.
  2. The snippet step is skipped for it; `buildEmbeddingText` emits
     `[Comment] <path>` + optional `context:` + the comment's text.
  3. The vector lands in `entity_vec`, mapped in `entity_vec_map` with
     `entity_type = 'Comment'`.
  4. A semantic or hybrid query matches on meaning, and RRF fuses it with the FTS hit
     the same comment already produced.
- **Alternative Flows**: with vectors missing, hybrid degrades to FTS-only and
  comments still match on keywords — the pre-existing behaviour.
- **Error Scenarios**: `EmbedLabelsForLang` returns nothing for the language (not
  declared) → the comment is keyword-only, exactly as before this change.
- **Postconditions**: comment prose is reachable by both halves of hybrid search.
- **Affected Files**: `internal/ast/embedder.go`, `internal/ast/fts_sqlite.go`
  (unchanged, consumes what the embedder produced).

### UC-03: Two labels claim the same entity, and the grammar breaks the tie
- **Actor**: the embedding cycle.
- **Preconditions**: one `(path, uid)` appears under two labels the language embeds.
- **Main Flow**:
  1. Both rows are collected, each carrying its own `Rank`.
  2. `labelOrder` sorts the buckets by best rank, then by name.
  3. The dedup walks that order and the first label to claim `path\x00uid` keeps it.
- **Error Scenarios**: labels tie on rank (impossible within one language, since rank
  is a list index) → the name breaks the tie, so the order is still deterministic.
- **Postconditions**: exactly one vector per `(path, uid)`, under the label its
  grammar ranked first.
- **Affected Files**: `internal/ast/embedder.go`.

## Test Cases & Acceptance Criteria

### Feature: Grammar-declared embed labels
Ref: UC-01

#### Scenario: Every shipped grammar declares its embeddable labels
```gherkin
Given the 45 query YAML files in internal/ast/queries
When TestEveryShippedGrammarDeclaresEmbedLabels parses each one
Then every file has a non-empty embed_labels list
```

#### Scenario: A declared label must be one the grammar can emit
```gherkin
Given a grammar whose queries emit Function, Test, Constant and Field
  And whose comment_types declares "comment"
When TestEmbedLabelsAreLabelsTheGrammarProduces checks its embed_labels
Then every declared label is among those produced or is Comment
  And a label such as "Variable", which no query of that grammar emits, fails the test
```

#### Scenario: An undeclared language embeds nothing
```gherkin
Given no query file anywhere declares the language "no-such-language-anywhere"
When EmbedLabelsForLang is called for it
Then it returns an empty list
  And no entity of that language is ever collected for embedding
```

#### Scenario: A project-level grammar decides for its own language
```gherkin
Given a project directory with .graphit/ast/queries/embedlabelslang.yaml
  And that file declares embed_labels of Function then Comment
When EmbedLabelsForLang is called with that project directory and that language
Then it returns exactly [Function, Comment], in that order
```

### Feature: Comments are embedded
Ref: UC-02

#### Scenario: A comment gets a vector alongside the declarations
```gherkin
Given a shard holding two Functions, a Variable and a Comment
  And a grammar declaring embed_labels of Function, Variable, Comment
When the embedding cycle runs
Then CountPending reports 4
  And RunCycle embeds 4
  And every one of the four UIDs has a vector in the embedding cache
```

#### Scenario: A comment's embedding text does not repeat its own source line
```gherkin
Given a file whose line 2 is "// documents Foo"
  And a Comment entity at line 2 named "documents Foo"
When the embedding cycle builds its text
Then the text starts with "[Comment]"
  And it contains "documents Foo"
  And it does not contain "// documents Foo"
```

#### Scenario: Every other label still carries its source snippet
```gherkin
Given a Function spanning lines 3 to 5 of the same file
When the embedding cycle builds its text
Then the text contains the sliced body "func Foo() {\n\treturn\n}"
```

### Feature: Collision ordering follows the grammar
Ref: UC-03

#### Scenario: The label the grammar listed first keeps the entity
```gherkin
Given a grammar declaring embed_labels of Class then Interface
  And a shard where uid "a.ts::Point" appears as Interface first and then as Class
When the embedding cycle runs
Then exactly one row is embedded
  And its embedding text begins with "[Class] "
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/query_loader.go` | Modified | Added `EmbedLabels` to `ExternalQueryFile`; merge rule; `hasLangConfig`; `EmbedLabelsForLang` + `firstDeclaredEmbedLabels` + `embedLabelCache`; cache invalidation |
| `internal/ast/embedder.go` | Modified | Removed `embeddableLabels`; added `embedRanker`, `entityRow.Rank`, `labelOrder`, `EmbeddingConfig.ProjectDir`; per-`(lang, label)` filter; comment snippet skip |
| `cmd/graphit/commands/ast.go` | Modified | `ast embed` sets `cfg.ProjectDir` |
| `internal/ast/queries/*.yaml` (45 files) | Modified | Each grammar declares its own `embed_labels` |
| `internal/ast/embed_labels_coverage_test.go` | Created | The two guard tests, plus `stageEmbedLabelsIn` / `stageEmbedLabelsGrammar` used by the embedder tests |
| `internal/ast/embedder_test.go` | Modified | `TestEmbeddableLabels` (asserted the deleted slice) → `TestEmbedLabelsResolvePerLanguage` |
| `internal/ast/embedder_streaming_test.go` | Modified | Both tests stage their own grammar and set `Lang`; comments now expected to embed; snippet-skip assertions |
| `internal/ast/embedder_no_source_test.go` | Modified | Fixture stages its grammar into the repo root and sets `ProjectDir` and `Lang` |
| `docs/specs/ast_module.md` | Modified | `embed_labels` in the YAML example, the field table and the merge table; "what gets a vector is the grammar's decision" under Semantic Vector Search |

## Trade-offs & Decisions

**No built-in default when `embed_labels` is absent.** A default would make omission
invisible, and it would be a list of labels in Go — the exact thing this task
removed, reintroduced as a fallback. Absent therefore means *embeds nothing*, and
`TestEveryShippedGrammarDeclaresEmbedLabels` is what stops that from being a silent
state for the grammars we ship. It stays a silent state for third-party grammars; see
Technical Debt.

**Rank comes from the list index, not from a separate priority field.** Order in a
YAML list is already an author's statement of priority, and it needs no new syntax.
The cost is that reordering the list changes collision behaviour — documented in the
field table and in the Go doc comment, since it is not obvious from the YAML alone.

**`ProjectDir` added rather than reusing `RepoRoot`.** One more field against a
supported state (`RepoRoot: ""`) that would otherwise have silently meant "embed
nothing". The test that blanks `RepoRoot` is what surfaced this — it went red and was
right to.

**Curated per-grammar lists, not "every label the grammar emits".** Embedding every
label would put `Parameter`, `Value` and `AttributeValue` in the vector index: high
volume, low meaning, and they crowd real matches out of the RRF ranking. The
selection is a judgement call per grammar and is now the grammar's to revise, which
was the point of the change.

**Comments are embedded without a length or quality gate.** No "skip comments under N
characters", no "skip commented-out code". A gate would be another rule in Go
deciding for content it cannot see; if a corpus wants comments out, its grammar drops
`Comment` from its list. The cost is real and stated below.

## Technical Debt

- [ ] **A third-party grammar that omits `embed_labels` is silent.** The guard tests
      cover `internal/ast/queries/*.yaml` only. A Hub `language` artifact or an
      `ast.queries_dir` file that forgets the field embeds nothing and nothing says
      so. A load-time warning in `parseQueryFile` — "language X declares no
      embed_labels; it will not be semantically searchable" — would close it, and
      matches how this project already handles a broken query pattern (warn, do not
      fail). Not done here to keep the change to one decision.
- [ ] **The embedding cost of this change is un-measured on a real corpus.** This
      repository alone holds ~15,100 `Comment` nodes against 8,695 vectors today, and
      the per-grammar lists also add labels the old sixteen never covered. At 768
      float32 per vector that is roughly 3 KB each before sqlite-vec's 1024-slot
      chunking. Both index size and cycle duration should be measured after the first
      full re-embed, and the per-grammar lists trimmed if the ranking degrades.
- [ ] **Comments may crowd declarations out of hybrid results.** RRF fuses FTS and
      semantic without knowing that a comment *about* `ProcessOrder` is not
      `ProcessOrder`. If this shows up, the fix is a rank weight per label in the
      fusion, not removing comments from the index.
- [ ] **End-to-end verification is still pending on this machine** — see System
      Knowledge. Unit-level behaviour is covered; a live re-embed and a semantic
      query returning a comment were not run.

## System Knowledge

**The parse cache is silently discarded on a version mismatch, and it looks like an
empty project.** `NewShardCache` reads `manifest.json` and keeps it only when
`loaded.Version == shardCacheVersion` (`internal/ast/shard_cache.go:113`) —
otherwise it proceeds with an empty `Files` map and no error. On this machine the
store's manifest is `v6` while the source tree is at `shardCacheVersion = 7`
(committed in `de37250`), so a binary built from source sees **zero** cached files.

Then **"✓ All entities up to date"** is reported in 0.07 seconds by `graphit ast embed`, because
`CountPending` legitimately returns 0 for an empty cache. That success message is the same one a genuinely up-to-date store produces. Diagnosis: compare `v` in `<store>/manifest.json` against `shardCacheVersion`. This is pre-existing and independent of this task — `make install` plus a reindex clears it. Related:
When parser output changes, bump `shardCacheVersion`; the cache is keyed by content hash.

The Query YAMLs reach a running binary through the extracted runtime, not the source tree. Editing `internal/ast/queries/*.yaml` changes nothing for an installed binary until the launcher re-extracts them to `~/.graphit/runtime/<version>/ast/queries` (the Makefile copies them into `cmd/launcher/runtime/ast/queries` at build time). Tests read that same extracted directory, which is why every embedder test here stages its own project-level grammar for an invented language instead of asserting on `go` — asserting on a shipped language would be asserting on whatever this machine last installed. Related:
Query YAML files are loaded from the extracted runtime rather than embedded in the internal AST binary.

**A property binds in Cypher when any candidate label has it.** Not relevant to the
code changed here, but confirmed again while reading the schema: `Comment` carries
`value` like the other 28 entity labels, and it is always empty — both adapters put
the comment's text in `Name`. Any future work that goes looking for comment text in
`value` is looking in the wrong column.

**`is_exported` is set on `Comment` nodes.** Visible in the shard JSON
(`"IsExported": true` on a comment row). Anything filtering "the public API" by
`is_exported` alone gets the file's comments too — the skill already warns about
this, and the shard confirms it.

## Progress Log

### 2026-08-16
- Measured the actual state before changing anything: comments already in `entity_fts`
  (verified by an `fts`-mode search returning `type=comment`), absent from
  `entity_vec` (verified by a `semantic`-mode search returning none, and by
  `GROUP BY entity_type` on the live `entity_vec_map`). Corrected the premise that
  comment text lives in the `value` property — it is the `name`.
- First implementation added `Comment` to `embeddableLabels`, with the snippet skip.
  Rejected by the Engineer: the fixed list is the defect, not its contents.
- Reimplemented as `embed_labels` per grammar: new field, merge rule,
  `EmbedLabelsForLang`, `embedRanker`, `labelOrder`, `EmbeddingConfig.ProjectDir`.
- Declared `embed_labels` in all 45 shipped grammars. The new
  `TestEmbedLabelsAreLabelsTheGrammarProduces` immediately caught a label I had
  invented (`Variable` in `zig.yaml`, which zig's queries do not emit) — the guard
  paid for itself before the change landed.
- Rewrote the embedder tests to stage their own grammar, which also removed their
  hidden dependency on the installed runtime's YAMLs.
- `go build ./...` clean; `go test -tags fts5 ./internal/ast/` green (56 s).
- **Not verified end to end**: a live re-embed and a semantic query returning a
  comment. Blocked on `make install` (needs sudo for `/usr/local/bin`) plus a reindex
  to rewrite the v6 store at v7. Everything below that line is covered by unit tests.
