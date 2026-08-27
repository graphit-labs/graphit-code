---
title: Unbind markdown from the AST index, and exclude the framework's lockfile
status: done
created: 2026-08-15
updated: 2026-08-15
tags: [ast, ignore, grammar, markdown, lockfile]
---

# Unbind markdown from the AST index, and exclude the framework's lockfile

## Objective

Three things were asked of the AST index, and the first one has a boundary that is
easy to overstep:

1. **The markdown grammar must not be used to parse for the AST.** Documents are the
   knowledge wiki's; in the code graph they were a `File` node per page and a
   `Heading` node per section.
2. **The framework's own lockfile must not be indexed.**
3. **`.<brand>/` must not be indexed.**

The boundary on (1): *not used by the AST* is not *removed from the framework*. If
the grammar supports any other functionality, or could, it stays. So the change is
the **binding**, not the grammar — and the first attempt at this task got that wrong
and deleted the vendored grammar, which is recorded under Trade-offs because the
distinction is the whole point of the task.

(3) turned out to be already true, twice over, and is documented below rather than
implemented.

## Implementation Details

### 1. Markdown: the query file goes, the grammar stays

Extensions are granted by a **query file**, not by a grammar. `internal/ast/query_loader.go`
builds the extension table from the `extensions:` field of the resolved YAML files, so a
grammar registered in `nativeGrammars` with no query file reaching it is never dispatched to.
That makes deleting one file the whole of "not parsed for the AST":

- **Deleted** `internal/ast/queries/markdown.yaml` — the only thing that claimed
  `.md`, `.markdown` and `.mdx`.
- **Kept** `internal/ast/treesitter/markdown/` (2.1 MB of vendored `parser.c.inc`,
  compiled through cgo from `binding.go`), its `nativeGrammars["markdown"]` entry, and
  its slot in the Makefile's `GRAMMAR_VENDORED`.

Keeping the registration is not inertia — it is what makes the opt-in work. The
resolution chain is project → user → runtime, so a project that wants markdown
structure writes its own `markdown.yaml` into `ast.queries_dir`; an override cannot
resolve a grammar that is not registered.

`internal/ast/writer.go:collectFiles` gates on `HasParserForExtensionIn`, and
`internal/daemon/syncmodule.go:classifyBatch` gates on the same function, so the full
scan and the incremental path agree without either being told about markdown
specifically.

### 2. The lockfile joins the AST default ignore patterns

`internal/ast/astignore.go`, `DefaultAstIgnorePatterns`:

```go
var DefaultAstIgnorePatterns = []string{
	brand.DotDir() + "/",
	"/" + brand.LockFileName(),
}
```

`graphit.lock.json` is the same category of file as a shard — the framework's own
state, `.json`, with a parser in front of it — written in the one place the brand-dir
default cannot reach. Before this it contributed `Pair` and `Value` nodes describing
the indexer to itself, and it is rewritten on every install, sync and config write, so
it churned on a schedule unrelated to the code.

**The leading slash is load-bearing.** A gitignore pattern with no internal slash
matches at any depth, which would also have excluded the lockfile of a fixture project
or a nested checkout. Those belong to whatever is being indexed, not to us.

### 3. `.<brand>/` — already excluded, in two independent places

No change was needed, and both mechanisms matter because they cover different paths:

| Path | Mechanism |
|---|---|
| Full scan | `collectFiles` returns `filepath.SkipDir` for **every** dot-directory (`internal/ast/writer.go:176`) |
| Incremental / watcher | `DefaultAstIgnorePatterns` carries `brand.DotDir() + "/"`; `classifyBatch` also drops anything under a dot-dir via `insideDotDir` |

Verified against the live graph: `MATCH (f:File) WHERE f.path STARTS WITH '.' OR f.path CONTAINS '/.'`
returns exactly one row, `.golangci.yml` — a dot**file** at the root, which is source and
should be there.

## Use Cases

### UC-01: A full index run skips every markdown file
- **Actor**: developer or daemon running `graphit ast index`
- **Preconditions**: no `markdown.yaml` resolves through the chain (project, user, runtime)
- **Main Flow**:
  1. `collectFiles` walks the tree (`internal/ast/writer.go:158`)
  2. For each file it calls `HasParserForExtensionIn(rootPath, ext)`
  3. `.md` resolves no query file, so the extension table has no entry and the file is
     not appended
- **Alternative Flows**: the project ships its own `markdown.yaml` under
  `ast.queries_dir` → the extension is registered for that project and markdown is
  indexed there (UC-04)
- **Error Scenarios**: an installed runtime predating this change still holds
  `markdown.yaml`, so `.md` stays registered until the next `make install` / launcher
  extraction. Symptom: markdown still in the graph. Fix: reinstall, or delete
  `~/.graphit/runtime/<version>/ast/queries/markdown.yaml`.
- **Postconditions**: no `File` node with `lang = 'markdown'`, and no `Heading` /
  `CodeBlock` node from a document
- **Affected Files**: `internal/ast/queries/markdown.yaml` (deleted),
  `internal/ast/writer.go`, `internal/ast/query_loader.go`

### UC-02: An incremental update of a document reaches the wiki alone
- **Actor**: the daemon's sync module, on a filesystem event
- **Preconditions**: the daemon is watching; the file is under `knowledge.docs_dir` or
  named by the wiki scope
- **Main Flow**:
  1. `classifyBatch` (`internal/daemon/syncmodule.go:149`) receives the path
  2. The knowledge branch matches: the extension is in `knowledge.extensions` and the
     path is in scope → `targets.knowledge = true`
  3. The AST branch calls `HasParserForExtensionIn`, gets false, and `continue`s
  4. `handleBatch` dispatches the wiki rebuild only
- **Alternative Flows**: a markdown file **outside** the wiki scope
  (`CONTRIBUTING.md`, `internal/x/README.md`) matches neither branch and wakes nobody
- **Error Scenarios**: as UC-01 — a stale runtime re-registers `.md` and the AST branch
  claims the file again
- **Postconditions**: the wiki reflects the edit; the code graph is untouched
- **Affected Files**: `internal/daemon/syncmodule.go`

### UC-03: The framework's lockfile is never indexed
- **Actor**: any index run, full or incremental
- **Preconditions**: none — the pattern is a built-in default
- **Main Flow**:
  1. `NewAstIgnoreChecker(root)` assembles `.gitignore` → `.astignore` → defaults
  2. `/graphit.lock.json` is appended last, so under last-match-wins it outranks
     anything the project wrote
  3. `IsIgnored("graphit.lock.json", false)` returns true and the file is dropped
- **Alternative Flows**: a lockfile deeper in the tree
  (`internal/hub/testdata/proj/graphit.lock.json`) is **not** matched — the pattern is
  anchored
- **Error Scenarios**: a project cannot re-include it with `!graphit.lock.json`, because
  defaults are applied last. That is intentional and matches the brand-dir entry.
- **Postconditions**: no `Pair` or `Value` node sourced from the lockfile; reading it
  for configuration (`config.LoadProjectConfig`) is unaffected — ignoring is about
  indexing, not about opening
- **Affected Files**: `internal/ast/astignore.go`

### UC-04: A project opts markdown back in
- **Actor**: developer who wants markdown structure in their own code graph
- **Preconditions**: `tree-sitter-markdown` is registered in `nativeGrammars` — it is
- **Main Flow**:
  1. Write `markdown.yaml` into `ast.queries_dir` (`.graphit/ast/queries/` by default)
     with `language: markdown`, `grammar: tree-sitter-markdown`, `extensions: [".md"]`
  2. The project level wins the resolution chain for that language
  3. `resolveTreeSitterLang("markdown", "tree-sitter-markdown")` resolves the compiled
     grammar
  4. `.md` files are discovered and produce `Heading` / `CodeBlock` nodes
- **Alternative Flows**: the same file at `~/.graphit/ast/queries/` enables it for every
  project on the machine
- **Error Scenarios**: a query file naming a grammar with no `nativeGrammars` entry
  resolves nothing and **fails silently** — which is exactly why the registration was
  kept
- **Postconditions**: markdown is indexed for that project only; the default is unchanged
- **Affected Files**: `internal/ast/treesitter_native.go`,
  `internal/ast/treesitter/markdown/`, `internal/ast/query_loader.go`

## Test Cases & Acceptance Criteria

### Feature: Markdown is not an AST language by default
Ref: UC-01

#### Scenario: no shipped query file claims a markdown extension
```gherkin
Given the query files in internal/ast/queries/
When each one is parsed and its extensions read
Then none of them declares ".md", ".markdown" or ".mdx"
```

#### Scenario: the grammar survives the unbinding
```gherkin
Given no shipped query file claims a markdown extension
When nativeGrammars is inspected
Then it still carries an entry for "markdown"
  And the opt-in in UC-04 therefore still has a grammar to resolve
```

`TestNoQueryFileClaimsMarkdown` (`internal/ast/css_test.go`) covers both. It reads the
**source tree**, so it does not depend on what the launcher last unpacked.

#### Scenario: a grammar with no query file is still a failure everywhere else
```gherkin
Given a grammar registered in nativeGrammars
When it is not named by any query file
  And it is not listed in grammarsWithoutDefaultQueries with a reason
Then TestEveryNativeGrammarHasQueries fails
```

#### Scenario: an exemption that outlives its grammar is a failure too
```gherkin
Given "markdown" is listed in grammarsWithoutDefaultQueries
When it is no longer present in nativeGrammars
Then TestEveryNativeGrammarHasQueries fails, naming the stale exemption
```

### Feature: A project can opt markdown back in
Ref: UC-04

#### Scenario: a project query file resolves the registered grammar
```gherkin
Given a project with its own markdown.yaml under ast.queries_dir
  And that file declares grammar tree-sitter-markdown and extension ".md"
When "# Title\n\n```go\nx := 1\n```\n" is parsed as doc.md
Then a CodeBlock entity is extracted
  And its context is the Heading "Title"
  And no CONTAINS edge dangles
```

`TestMarkdownContentBelongsToItsHeadingWhenAProjectOptsIn`
(`internal/ast/generic_callable_container_test.go`). It stages the YAML **inline** via
the new `stageGrammarWithQueries` helper rather than reading `queries/markdown.yaml`,
which no longer exists — reading it would turn a broken opt-in into a silent skip.

### Feature: The framework's lockfile is excluded from the code graph
Ref: UC-03

#### Scenario Outline: what the anchored pattern matches
```gherkin
Given a project with no .gitignore and no .astignore
When the AST ignore checker is asked about "<path>"
Then it reports ignored = <ignored>

Examples:
  | path                                        | ignored | why                               |
  | graphit.lock.json                           | true    | the project's own lockfile        |
  | internal/hub/testdata/proj/graphit.lock.json| false   | not ours — anchored to the root   |
  | graphit.lock.json.bak                       | false   | a different file                  |
  | graphit.lock.json.tmpl                      | false   | a different file                  |
```

`TestAstIgnoreCheckerExcludesTheFrameworkLockfileByDefault`
(`internal/ast/astignore_test.go`).

#### Scenario: the brand directory is still excluded alongside it
```gherkin
Given a project with no .gitignore and no .astignore
When the checker is asked about ".graphit" as a directory
  And about ".graphit/ast/project/shards/a.sql.nodes.json"
Then both are ignored
  And "a.sql", "src/b.go" and ".hidden.sql" are not
```

Pinned by the pre-existing `TestAstIgnoreCheckerExcludesBrandDirByDefault`, unchanged.

### Feature: Routing agrees with the full scan
Ref: UC-02

#### Scenario: a structured document under docs reaches both indexers
```gherkin
Given knowledge.docs_dir is "docs"
When docs/openapi.yaml changes
Then the AST is handed the file
  And a knowledge rebuild is scheduled
```

#### Scenario: markdown outside the docs dir reaches neither indexer
```gherkin
Given knowledge.docs_dir is "docs"
When CONTRIBUTING.md changes
Then the AST is handed nothing
  And no knowledge rebuild is scheduled
```

#### Scenario: the root README still rebuilds the wiki, and only the wiki
```gherkin
Given knowledge.docs_dir is "docs"
  And the wiki scope names README.md
When README.md changes
Then a knowledge rebuild is scheduled
  And the AST is handed nothing
```

`TestClassifyBatch` (`internal/daemon/syncmodule_classify_test.go`). Its
`requireParsers` guard skips the whole table when the **installed runtime** disagrees
with the source tree — in either direction: `.sql`/`.go`/`.yaml` missing means no
runtime at all, and `.md` present means a runtime older than this change. Neither is a
routing bug, and failing on either would say it was.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/queries/markdown.yaml` | Deleted | The only thing binding `.md`/`.markdown`/`.mdx` to the pipeline |
| `internal/ast/astignore.go` | Modified | `DefaultAstIgnorePatterns` gains the anchored `/graphit.lock.json` |
| `internal/ast/astignore_test.go` | Modified | New `TestAstIgnoreCheckerExcludesTheFrameworkLockfileByDefault`, anchoring included |
| `internal/ast/css_test.go` | Modified | New `grammarsWithoutDefaultQueries` exemption map, both directions checked in `TestEveryNativeGrammarHasQueries`; new `TestNoQueryFileClaimsMarkdown` |
| `internal/ast/data_format_kv_test.go` | Modified | `stageGrammar` split so `stageGrammarWithQueries` can stage a query file the framework does not ship |
| `internal/ast/generic_callable_container_test.go` | Modified | The markdown test became the opt-in test, with its YAML inline |
| `internal/ast/containment_coverage_test.go` | Modified | Comment no longer cites markdown among the grammars this check covers |
| `internal/daemon/syncmodule.go` | Modified | Two comments named `.md` as an AST/wiki overlap and used `docs/guia.md` as the example |
| `internal/daemon/syncmodule_classify_test.go` | Modified | Overlap case moved to `.yaml`; the two markdown cases now expect the wiki alone; `requireParsers` gained the stale-runtime guard |
| `internal/daemon/syncmodule_wikiwatch_test.go` | Modified | Header comment: these tests stage their own `.md` parser, which is why `.md` still routes here |
| `docs/specs/ast_module.md` | Modified | Language table drops the Markdown row and renumbers; counts 45→44 and 40→39; a note on why markdown is absent and how to opt in; File Discovery now names three exclusions |
| `docs/guides/ignore_files.md` | Modified | Third default row; markdown is out by the language set, not by a pattern; `ast.index_docs` returns the directory, not markdown |
| `docs/guides/troubleshooting.md` | Modified | Supported-language list and counts; where documents are searched instead |
| `docs/guides/user_manual.md` | Modified | Grammar count stays 40 while default query files are 44; a query file can add a language, with markdown as the worked example |
| `README.md` | Modified | Language list and count |

Not changed, deliberately: `internal/ast/treesitter/markdown/**`,
`nativeGrammars["markdown"]`, and `GRAMMAR_VENDORED` in the `Makefile`.

## Trade-offs & Decisions

**Unbind the language, do not remove the grammar.** The first pass deleted the
vendored grammar, dropped the `nativeGrammars` entry and removed markdown from
`GRAMMAR_VENDORED` — reverted on instruction. The reasoning that makes the correction
stick, rather than it being a preference: *registered* and *indexed* are different
questions, exactly as *versioned* and *indexed* are for the brand directory. A grammar
is a capability; a query file is a decision to use it. Removing the capability also
removes the opt-in and any other use, present or future, and buys nothing the deletion
of one YAML file did not already buy.

**Counts in the docs had to split.** "45 languages via 40 Tree-sitter grammars" could
be decremented as one number while the grammar was leaving with the query file. It
cannot now: **40** grammars are compiled in, **44** languages are indexed, **39** of
them through Tree-sitter. Any doc that conflates the two is wrong the moment a grammar
is unbound, which is why the wording changed and not just the digits.

**The lockfile pattern is anchored; the brand-dir pattern is not.** Deliberately
asymmetric. `.graphit/` should match at any depth — a nested checkout's brand directory
is still machine state nobody wants in a code graph. A nested `graphit.lock.json` is
that project's declaration of its own contexts, and is legitimately indexable content
for whoever is indexing it.

**`TestClassifyBatch` skips instead of failing on a stale runtime.** The extension table
comes from `~/.graphit/runtime/<version>/ast/queries/`, not from the source tree, so
that test cannot assert this change until the launcher re-extracts. A skip that names
the reason beats a failure that points at routing logic which is correct. The
source-tree assertion lives in `internal/ast` (`TestNoQueryFileClaimsMarkdown`), where
it is deterministic. Verified by hand both ways: with `markdown.yaml` present in the
runtime the guard skips; with it removed all twelve subtests pass.

## Technical Debt

- [ ] `internal/ast/queries/` holds 47 YAML files while the docs say 44 languages — the
      ANTLR dialects add rows the language table counts differently. Pre-existing; the
      45→44 delta is right, the absolute number was already approximate.
- [ ] `docs/specs/embedded_language_parsing.md` still cites Markdown as a candidate for
Embedded Parsing (Enable in a language that doesn't have blocks — Astro)
      Markdown, Handlebars"). Still technically true — the grammar is there — but a
      reader could take it as evidence markdown is indexed. Left alone rather than
      rewritten from an adjacent task.
- [ ] `docs/guides/grammar_build_distribution.md` opens with "42 programming languages".
      Already stale before this change; not touched, to keep the diff about one thing.

## System Knowledge

- **A query file, not a grammar, registers an extension.** `internal/ast/query_loader.go`
  builds the extension table from `extensions:`. So the answer to "stop indexing
  language X" is to remove the YAML, and the answer to "keep the capability" is to leave
  the grammar registered. The two are independent, and conflating them is what the first
  pass got wrong.
- **The reverse is a silent failure and has bitten this repo before.** A query file whose
  `grammar` has no `nativeGrammars` entry resolves nothing, registers nothing, and
  reports nothing — the shape of the CSS and Svelte omissions that
  `TestEveryNativeGrammarHasQueries` exists to catch.
- **Query YAMLs are not `go:embed`ed.** `internal/ast/queries/*.yaml` is copied to
  `cmd/launcher/runtime/ast/queries/` by the Makefile, embedded into the launcher, and
  extracted to `~/.graphit/runtime/<version>/ast/queries/`. A plain
  `go build ./cmd/graphit` reads the **previous** extraction, which is why a test
  routing on extensions can disagree with the source tree it was built from.
- **Default ignore patterns are appended last and therefore win.** `internal/ignorer/ignorer.go:New`
  orders `.gitignore` → custom file → defaults, and go-git's matcher iterates backwards
  returning the first match. A `!` negation in `.astignore` cannot re-include anything a
  default excludes — which is why an adjustable exclusion needs a config key
  (`ast.index_docs`) and an inviolable one does not.
- **The brand directory is excluded twice, by unrelated code.** `collectFiles` skips all
  dot-directories on a full scan; `DefaultAstIgnorePatterns` and `insideDotDir` cover the
  incremental path. Either alone would leave a hole, since the two paths do not share a
  discovery step.
- **The watcher union is not the ignore decision.** `ignoreUnion.IsIgnored` skips a path
  only when *every* member skips it, so adding the lockfile to the AST defaults does not
  stop the watcher seeing it — `classifyBatch` then applies each consumer's own checker.
  Narrowing one module's ignore file cannot silently deafen the other.

## Progress Log

### 2026-08-15
- Searched project memory first: found the ordering of default patterns
  (defaults last, so negation cannot override), the brand-dir split by ownership, and
  the note that `.astignore`/`.wikiignore` exclude the whole brand directory on purpose.
- Confirmed the starting state from the live graph: `graphit.lock.json` was indexed as
  JSON; 38 markdown files were indexed (root community files, `tasks/`,
  `internal/memory/index.md`); nothing from `.graphit/`.
- First pass removed the markdown grammar wholesale. **Corrected on instruction**: the
  grammar is not to be removed from the framework, only unbound from AST parsing.
  Restored `internal/ast/treesitter/markdown/`, the `nativeGrammars` entry and the
  Makefile line; kept only the query file deleted.
- Added `grammarsWithoutDefaultQueries` so the orphan check stays strict for every other
  grammar, and checked it in both directions.
- Turned the deleted markdown test into the opt-in test, which is now the only thing
  proving the retained grammar still resolves.
- `go build ./...`, `go vet`, `make lint` (0 issues), and
  `go test ./internal/{ast,daemon,ignorer,knowledge,brand,config}/` all green.
- Verified `TestClassifyBatch` both ways against the installed runtime.
