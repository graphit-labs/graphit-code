# `knowledge.docs_dir` defaults to `docs`, the AST stops indexing it, and the root README joins the wiki

## Date
2026-08-12

## Problem
`knowledge.docs_dir` defaulted to `"."`, the project root. Three consequences, all
of them wrong by default:

1. **The wiki indexed the whole repository.** Every file with one of the 16
   knowledge extensions became a wiki page — vendored markdown, generated JSON,
   root-level `tasks/`, anything `.wikiignore` did not happen to name. The wiki is
   documentation; it was a file listing.
2. **The agent-facing skill told agents to write into `./`.** `KnowledgeRuleContent`
   substitutes the configured docs dir into its own text, so with `"."` the
   generated skill said "document this in `./architecture/`" and "the task log in
   `./tasks/`". Those directories do not exist.
3. **The AST graph carried the documentation tree as well**, since the AST pipeline
   has parsers for markdown, YAML, JSON and XML. A `File` node per page and a
   `Heading` node per section, in a graph meant for code.

Changing the default alone would have dropped the project's `README.md` out of the
wiki: it is by convention *not* inside the docs tree, and it is the one page a
reader reaches for first.

## Root Cause
`config.ResolveDocsDir` returned `"."` when the key was unset, and both indexers
read that one value with no notion that a documentation tree is a *subtree* rather
than the root: `RunIndexPipeline` was handed the docs path **as** its root, so
there was nowhere for a document outside it to come from, and `NewAstIgnoreChecker`
knew nothing about configuration at all.

## Changes

### `internal/config/config.go`
- `DefaultDocsDir = "docs"`; `ResolveDocsDir` falls back to it instead of `"."`.
- `ResolveKnowledgeIncludeReadme` — `knowledge.include_readme`, default **true**.
- `ResolveAstIndexDocs` — `ast.index_docs`, default **false**.
- `LoadProjectConfig(projectDir)` reads the `config` object out of a lockfile with
  `encoding/json`. This duplicates a sliver of `hub.LoadLockfile` deliberately:
  `hub` imports `ast`, so `ast` cannot import `hub`, and the AST side needs project
  configuration to decide whether the docs tree is its business.

### `internal/ast/astignore.go`
- `AstIgnorePatternsFor(rootPath)` and `DocsIgnorePatternFor(rootPath)`: the docs
  tree is appended to the default ignore patterns unless `ast.index_docs` is `true`.
  `NewAstIgnoreChecker`'s signature is unchanged, so all four call sites — the
  watcher, `collectFiles`, the daemon's `SyncModule`, `watchAndReindex` — pick it up
  with no plumbing.
- The pattern is **anchored** (`/docs/`, not `docs/`): a bare gitignore `docs/`
  means "any directory named docs at any depth" and would also swallow
  `internal/x/docs/`.
- Returns no pattern when `docs_dir` is `.` (excluding it would empty the graph) or
  when the path escapes the project (the checker matches project-relative paths, so
  no pattern could describe it).

### `internal/knowledge/wiki.go`
- New `WikiScope{Subdir, ExtraFiles}`. `GenerateKnowledgeWiki` takes it as a fifth
  parameter and `enumerateKnowledgeSources` walks `root/Subdir`, then adds
  `ExtraFiles` — de-duplicated by path, so `docs_dir=.` (where the walk already
  finds the README) indexes it once, not twice.
- **The root stays the project directory.** Every path the wiki reports — the
  `source:` frontmatter field, the `.manifest.json` entry, the process-cache key —
  is relative to the root, and passing `docs/` in as the root reported a spec as
  `specs/config_module.md`, a path relative to nowhere the reader is standing. It
  also broke ignore files: `.gitignore`/`.wikiignore` are collected upward and each
  is scoped to its own directory relative to the root, so with `docs/` as the root
  the project's own ignore files sat one level *above* it, got the domain `..` from
  `domainForFile`, and matched nothing. A project with a custom `docs_dir` had its
  root `.gitignore` silently inert for the wiki. Verified in both directions by
  `TestIgnoreFilesAtBothLevelsApply`.
- **Collection starts at the docs tree, resolution is against the project**
  (`NewKnowledgeIgnoreCheckerIn`, `internal/knowledge/knowledgeignore.go`). Getting
  the root right is only half of it: passing the project as the *start* directory
  too would have stopped reading a `.wikiignore` kept inside the docs tree, which
  the old arrangement did read because the docs tree was its start directory. The
  first draft of this change had exactly that regression; it was caught by probing
  old versus new behaviour rather than by reasoning about it. Both files are
  registered as `StatPreCheck` watch files so editing either invalidates the
  fast-path.
- A `Subdir` that does not exist is not an error — `filepath.Walk`'s root error is
  swallowed, so it yields no sources and `ExtraFiles` are still indexed. That is
  what gives a project with a README and no `docs/` a one-page wiki instead of an
  empty one.

### `internal/knowledge/scope.go` (new)
- `ScopeFor(projectDir, inlineCfg, projectCfg)` assembles the scope from config.
- `RootReadme(projectDir, exts)` reads the root directory and returns the first file
  whose base name is `readme` in any casing with an extension `knowledge.extensions`
  accepts — `README.md`, `readme.markdown`, `README.rst`, `README.adoc` — rather
  than probing the cross product of casing and extension as literal names. A
  `README.pdf` is not a document this pipeline can chunk, so it is not picked up.

### `internal/knowledge/indexer.go`
- `IndexConfig.Scope`. The zero value indexes everything under the root, which is
  what an imported context needs: its extracted docs tree already *is* the root.

### `internal/daemon/syncmodule.go`
- `reindexKnowledge` passes `m.projectDir` as the root with `ScopeFor`'s scope, and
  returns early only when the docs tree is missing **and** there are no extra files.
- `classifyBatch` takes `extraDocs`: a path is documentation when it is under the
  docs tree *or* is exactly one of the scope's extra files. Without this an edit to
  `README.md` rebuilt nothing.
- The watch is still the union of what both checkers want, so excluding the docs
  tree from the AST does not stop the wiki from hearing about it.

### Call sites
`cmd/graphit/commands/{knowledge,lifecycle,runners}.go`,
`internal/mcpstdio/tools_{knowledge,lifecycle}.go`. `runKnowledgeIndex` and
`runKnowledgeWatch` now take `(root, scope, …)`. An explicit path —
`graphit knowledge index documentation/`, or the MCP `path` input — is taken
literally and indexed wholesale with a zero scope, which is also how
`runKnowledgeImport` was already behaving.

### Agent-facing rule text
- `internal/knowledge/rule.go`: the `docsDir` fallback is `config.DefaultDocsDir`,
  and a new section — "What the wiki reads" — names the three keys and states that a
  document outside the scope was never indexed rather than indexed and lost.
- `internal/ast/rule.go`: the docs tree is named as the exclusion an agent will meet
  most often, with "ask the wiki" as the answer instead of falling back to grep.
- `internal/hub/rule.go`: the configuration-symptom table gains rows for all three
  keys; its `docs_dir` row said "defaults to `.`, the whole project".
- Regenerated `.claude/`, `.agents/` and `.kiro/` skills. The knowledge skill's diff
  is mostly `./` → `docs/`: consequence 2 above, fixed by the default change.

## Documentation
- `docs/specs/config_module.md` — the three keys in the table, plus a section each
  covering precedence, override syntax, and why two docs-dir shapes produce no AST
  exclusion. Also `LoadProjectConfig` and why it duplicates the hub.
- `docs/guides/ignore_files.md` — built-in defaults are applied last and therefore
  **outrank** the project's own patterns, so `!docs/` cannot override the exclusion;
  the AST default table (previously "no built-in default patterns", already stale
  before this change); what the knowledge module sees *before* ignore rules apply.
- `docs/specs/wiki_module.md` — the `WikiScope` contract and why the root is the
  project rather than the docs directory.
- `docs/specs/ast_module.md` — both built-in exclusions in File Discovery.
- `docs/specs/daemon_module.md` — routing, and the README as a scope member.
- `docs/specs/hub_collaboration.md` — the lockfile example had `docs_dir` at the top
  level (it is `config.knowledge.docs_dir`) and `modules.ast` as
  `{"disabled": false}` (values are strings). Both were inert as written.
- `docs/guides/troubleshooting.md` — two new entries: a document that exists but is
  not in the wiki, and documentation missing from the AST graph. Also
  `graphit config get docs.dir` → `knowledge.docs_dir`.
- `docs/guides/{user_manual,cli_reference}.md` — the default scope.

## Testing
`go build ./...` clean; `go vet -tags fts5` clean on every changed package.

New tests:
- `internal/config/config_default_test.go` — the default is `docs`; both new bools
  in all three states; `LoadProjectConfig` against absent, malformed and valid
  lockfiles.
- `internal/ast/astignore_test.go` — the docs tree is excluded, anchored (a nested
  `docs/` survives) and `dirOnly` (a *file* named `docs` survives);
  `ast.index_docs=true` puts it back; `docs_dir=.` excludes nothing; a custom nested
  docs dir is what gets excluded and `docs/` stops being special.
- `internal/knowledge/scope_test.go` — `TestIgnoreFilesAtBothLevelsApply`: a root
  `.gitignore` and a root `.wikiignore` now apply to the scoped build (neither did
  before); a `.wikiignore` inside the docs tree still applies; and a docs-level
  pattern stays scoped to the docs tree instead of matching the project README.
  Also scope defaults and overrides; README casing
  and extension variants, directory and unreadable-extension rejection; an
  end-to-end build proving `vendor/` and `internal/` markdown stay out while
  `README.md` and `docs/specs/feature.md` come in with **project-relative** source
  paths; `docs_dir=.` enumerating the README exactly once; a README-only project
  producing a non-empty wiki.

Updated tests:
- `internal/daemon/syncmodule_classify_test.go` — the root README routes to the wiki
  as well as the AST; a nested `README.md` does not.
- `internal/knowledge/knowledge_coverage_test.go` — the generated rule names
  `docs/`, all three keys, and `README.md`.
- The old `TestAstIgnoreCheckerExcludesBrandDirByDefault` asserted
  `docs/guia.md` *should not* be ignored. It now asserts the opposite, in a test of
  its own.

Suites run green with `-tags fts5` and `LD_LIBRARY_PATH` pointed at the LadybugDB
module: `internal/{config,knowledge,ast,daemon,hub,mcpstdio}`,
`cmd/graphit/commands`, `cmd/launcher`.

## Migration Notes
**Existing projects that relied on the old default will index less.** A project
whose documentation is not under `docs/` gets a wiki with only its README until it
sets `knowledge.docs_dir`. This is the intended break — the old default's breadth
was the defect — but it is a break.

In **this** repository it means the root `tasks/` directory (30 documents, distinct
from `docs/tasks/`) leaves the wiki. Moving those files under `docs/tasks/` is the
fix; setting `knowledge.docs_dir=.` would restore them at the cost of everything
described above.

Wiki pages for projects already using a custom `docs_dir` rebuild once, because
their cache keys and `source:` fields change from docs-relative to project-relative.
