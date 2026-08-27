# Ignore Files

Graphit uses ignore files to control which project files are indexed by the AST and Knowledge modules. These files follow `.gitignore` syntax and are layered on top of `.gitignore` rules.

## File Types

| File | Module | Purpose |
|---|---|---|
| `.gitignore` | All | Base layer — always loaded first |
| `.astignore` | AST | Controls which source files are parsed and indexed in the code graph |
| `.wikiignore` | Knowledge | Controls which documentation/source files are included in the knowledge wiki |

## How Layering Works

Rules are evaluated in order using **last match wins** semantics (same as git):

1. `.gitignore` patterns are loaded first (base layer)
2. `.astignore` or `.wikiignore` patterns are loaded after (custom layer)
3. Built-in default patterns are loaded last

The custom ignore file can:

- **Add** new ignore rules on top of `.gitignore`
- **Negate** (override) rules from `.gitignore` using `!` patterns

> ⚠️ **Built-in defaults are loaded last, so under last-match-wins they outrank
> everything the project writes.** This is deliberate — the `.graphit/` entry
> exists to stop the indexer from indexing its own output, and a project must not
> be able to break that by accident. The consequence is that a `!` negation in
> `.astignore` or `.wikiignore` **cannot** re-include something a default excludes.
> Where a default is meant to be adjustable, there is a config key for it; see
> [the docs tree](#the-docs-tree-is-excluded-from-the-ast-by-default) below.

### Example

```gitignore
# .gitignore
build/
vendor/
```

```gitignore
# .astignore
# Ignore all generated ANTLR code
internal/ast/antlr/

# But re-include specific files we need indexed
!internal/ast/antlr/common/
!internal/ast/antlr/*/driver.go
!internal/ast/antlr/*/parser_sll_ll.go

# This also overrides .gitignore — include vendor/ in AST
!vendor/internal/
```

## Syntax

Follows standard `.gitignore` syntax:

| Pattern | Meaning |
|---|---|
| `pattern` | Ignore files matching pattern |
| `pattern/` | Ignore directories matching pattern |
| `!pattern` | Negate — re-include previously ignored files |
| `#` | Comment line |
| `*` | Wildcard — matches any characters except `/` |
| `**` | Matches any number of directories |
| `?` | Matches a single character |

## Negation Patterns and Directory Traversal

When a directory is ignored but contains negated children, Graphit's walker **descends** into the directory instead of skipping it entirely. This is handled by `ShouldDescend` in the ignore checker.

Example: with `.astignore` containing:

```gitignore
internal/ast/antlr/
!internal/ast/antlr/*/driver.go
```

The walker will:

1. See `internal/ast/antlr/` is ignored
2. Check `ShouldDescend` — finds negation targeting children → enters the directory
3. Inside, each file is checked individually — `driver.go` files pass, everything else is ignored

## File Placement

Ignore files are collected by walking **up** from a start directory to the git root,
and each file's patterns are scoped to the directory it sits in ("its domain"),
relative to the root the checker resolves against:

- **Project root**: rules apply to every path
- **The scoped start directory**: rules apply within it — `rascunho.md` in
  `docs/.wikiignore` matches `docs/rascunho.md` and not `rascunho.md` at the root

For the Knowledge module the start directory is `knowledge.docs_dir`, so **both**
`.wikiignore` at the project root and `.wikiignore` inside the docs tree are read.
For the AST module the start directory is the indexed root, so `.astignore` is read
there.

> ⚠️ **Collection only walks upward.** An ignore file *deeper* than the start
> directory — `docs/specs/.wikiignore`, `internal/x/.astignore` — is never read.
> Patterns for a nested directory belong in the ignore file at the start
> directory or the root, written with the path in them (`specs/rascunhos/`).

## Default Patterns

### `.astignore`

The AST module has three built-in defaults; everything else is controlled by the `.astignore` file and supported file extensions.

| Pattern | Why | Adjustable |
|---|---|---|
| `.graphit/` | The framework's own directory inside the project it is indexing: caches, logs, grammar query YAMLs, rule overrides. Indexing its own output amplifies rather than merely wastes — a shard is a `.json` file and `.json` has a parser, so indexing a shard emits a shard for the shard, each round producing more files than the last. `graphit init` ignores part of it in git, but that file belongs to the user, so the guard cannot rest on it. | No |
| `/graphit.lock.json` | The same output, written outside that directory. It is the framework's own state, `.json` has a parser, and it is rewritten on every install, sync and config change — so it churns the graph with `Pair` and `Value` nodes describing the indexer to itself. Anchored with a leading slash: only the project's own lockfile is the framework's, and one inside a fixture or a nested checkout belongs to that project. | No |
| `knowledge.docs_dir` (default `docs/`) | The documentation tree belongs to the knowledge wiki. | Yes — `ast.index_docs` |

> **Markdown is excluded by the language set, not by a pattern.** No shipped query
> file claims `.md`, `.markdown` or `.mdx`, and extensions are what a query file
> grants, so a document is never discovered — no ignore pattern is consulted and
> `ast.index_docs` does not bring it back. The grammar itself is still compiled in,
> so a project that wants markdown structure opts in with its own `markdown.yaml`
> under `ast.queries_dir`. See
> [AST Module](../specs/ast_module.md#-supported-languages).

> **Ignored by the indexer is not the same as ignored by git.** The `.gitignore`
> block `graphit init` writes names `.graphit/runtime/` and `.graphit/grammars/`.
> Project query YAMLs under `.graphit/ast/queries/` and rule overrides remain visible
> to Git; generated outputs and local parser binaries do not. See
> [Git Module](../specs/git_module.md#the-generated-gitignore-block) and
> [Storage Layout](../architecture/storage_layout.md#inside-a-projects-brand-directory).
> The `.astignore` default above still keeps the **whole** brand directory out of the
> code graph, grammar YAMLs included, and that is intended: they configure the parser
> rather than being parsed. The two layers answer different questions, and a `!` line
> in `.astignore` cannot re-include what a built-in default excludes.

#### The docs tree is excluded from the AST by default

The AST pipeline has parsers for YAML, JSON, XML and Protobuf, so it used to index
the documentation tree as thoroughly as the wiki did — and while a markdown query
file still shipped, that meant a `File` node per page and a `Heading` node per
section, in a graph meant for code. That is noise in every structural query, and
on a large docs tree a real share of the index. The wiki chunks, links and ranks
prose in ways a code graph cannot, so the two no longer overlap. Unbinding markdown
closed the same gap for documents kept *outside* the docs tree, which this pattern
never covered: `README.md`, `CONTRIBUTING.md`, `AGENTS.md`.

The pattern is derived from `knowledge.docs_dir` and **anchored to the project
root** — `/docs/`, not `docs/` — so a `docs/` directory nested inside real source
is left alone:

```
docs/specs/feature.md          → excluded
internal/x/docs/nota.md        → indexed (nested, not the project docs tree)
```

To put the docs tree back in the code graph — for `.proto` or `.graphql` schemas
kept under `docs/`, say:

```bash
graphit config ast.index_docs true
```

This puts the *directory* back, not markdown: no shipped query file claims `.md`,
so what returns is the structured files under `docs/`. Markdown is a separate
opt-in — a `markdown.yaml` under `ast.queries_dir`.

**Use the key, not `.astignore`.** The exclusion is a built-in default, and
defaults outrank the project's own patterns, so `!docs/` in `.astignore` has no
effect. Two configurations produce no exclusion at all: `knowledge.docs_dir` set
to `.` (the docs tree is the project, and excluding it would empty the graph), and
a docs dir that escapes the project. Full detail in
[config_module](../specs/config_module.md).

#### Excluding a language rather than a path

`.astignore` answers "which paths", and a query file's `extensions:` answers "which
file types". Neither answers "not this language, anywhere" — a language's files are
scattered across the tree, and the query files that grant its extensions live in the
installed runtime directory, which is regenerated on every install and is not yours
to edit. That is the third axis, and it is configuration:

```bash
# take one language out of this project's graph
graphit config ast.grammars_blacklist yaml

# out of every project on this machine
graphit config --global ast.grammars_blacklist yaml

# or the other way round: index nothing but these
graphit config ast.grammars_whitelist go,sql
```

Both keys are comma-separated. A name matches the language, the grammar, or the
grammar without its `tree-sitter-` / `antlr-` prefix — so `yaml`, `yaml_lang` and
`tree-sitter-yaml` all name the same language. When the whitelist is non-empty it
is exhaustive, and the blacklist still subtracts from it. Semantics and precedence:
[config_module](../specs/config_module.md#turning-grammars-off-astgrammars_blacklist-and-astgrammars_whitelist);
enforcement and what happens to nodes already indexed:
[ast_module](../specs/ast_module.md#turning-a-grammar-off-by-configuration).

So the three axes, in the order a file meets them:

| Axis | Mechanism | Question it answers |
|---|---|---|
| Language | `ast.grammars_blacklist` / `ast.grammars_whitelist` | is this language indexed at all? |
| File type | a query file's `extensions:` | does anything parse this extension? |
| Path | `.gitignore`, `.astignore`, built-in defaults | is this location indexed? |

A file needs all three to say yes. When one of them says no, the others are never
consulted — which is why "I added `!docs/**/*.yaml` and it still is not indexed" is
usually a language or extension answer, not a pattern one.

### `.wikiignore`

The Knowledge module includes built-in defaults that ignore:

- **Binaries**: `*.exe`, `*.dll`, `*.so`, `*.dylib`, `*.class`, `*.jar`, etc.
- **Media**: `*.jpg`, `*.png`, `*.mp3`, `*.mp4`, etc.
- **Archives**: `*.zip`, `*.tar`, `*.gz`, etc.
- **Office files**: `*.pdf`, `*.doc`, `*.xls`, etc.
- **Minified files**: `*.min.js`, `*.min.css`, `*.map`
- **Lock files**: `package-lock.json`, `yarn.lock`, `Cargo.lock`, `go.sum`
- **Dependencies**: `node_modules/`, `vendor/`, `.venv/`, `__pycache__/`
- **Build outputs**: `dist/`, `build/`, `.cache/`, `coverage/`
- **IDE config**: `.idea/`, `.vscode/`, `.vs/`
- **Agent config**: `.agents/`, `.claude/`, `.cursor/`, `.gemini/`, etc.
- **Graphit internal**: `.graphit/`

Because defaults outrank project patterns, a `!` negation in `.wikiignore` will not
re-include any of these — the list itself is the contract.

## What the Knowledge Module Sees Before Ignore Rules Apply

`.wikiignore` narrows a set that is already narrow. The wiki walks
`knowledge.docs_dir` (default `docs/`) and adds the project root's README, and
nothing outside those two reaches the ignore checker at all:

| Path | In the wiki? | Why |
|---|---|---|
| `docs/specs/feature.md` | yes | under the docs tree |
| `README.md` | yes | the root README, indexed whatever `docs_dir` says |
| `docs/README.md` | yes | under the docs tree, and a distinct page from the root one |
| `internal/x/notas.md` | no | outside the scope — no ignore rule involved |
| `internal/x/README.md` | no | only the **root** README is in scope |

So a document that does not appear in the wiki is more often out of scope than
ignored. `knowledge.docs_dir` moves the tree, `knowledge.include_readme=false`
drops the README, and `knowledge.docs_dir=.` restores the pre-default behaviour of
indexing the whole project — at which point `.wikiignore` is doing the heavy
lifting again.

## Effect of Changes

When an ignore file is modified:

- **AST sync**: The next `graphit sync` or daemon cycle detects changed files. Files that are now ignored are removed from the graph database and shard cache (including `.nodes.json`, `.edges.json`, and `.emb.json` files). Files that are now included are parsed and added.

- **Knowledge sync**: The next `graphit sync` detects the ignore file mtime change (tracked via `StatPreCheck` watch files), forces a full walk, prunes removed files from the process cache, removes their wiki pages, and rebuilds the cross-reference graph, backlinks, communities, and the search index.

In both cases, empty shard directories are cleaned up automatically after file removal.
