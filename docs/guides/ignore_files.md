---
title: Ignore Files
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-84a0}
    relation: implements
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-ab16
    relation: supports
  - target:
      type: task
      id: tsk-b5d6
    relation: supports
  - target:
      type: task
      id: tsk-1fee
    relation: supports
  - target:
      type: task
      id: tsk-e079
    relation: supports
  - target:
      type: task
      id: tsk-8d62
    relation: supports
  - target:
      type: task
      id: tsk-6997
    relation: supports
---

# Ignore Files

Graphit uses `.gitignore`, `.astignore` and `.wikiignore` syntax to select files
for AST and Knowledge. The scope is the admitted project and version; a rule
file is configuration, not authority to read another project.

## File types and layering

| File | Purpose |
|---|---|
| `.gitignore` | Base repository rules |
| `.astignore` | AST source selection |
| `.wikiignore` | Knowledge document selection |

Defaults have priority, followed by custom files from deepest scope to root,
then `.gitignore` files from deepest scope to root. Within one source, the last
direct match wins. A custom negation can reopen a user-ignored path but cannot
reopen a matching default exclusion.

```gitignore
generated/*
!generated/driver.rs
```

`should_descend` enters only permitted directories. A file negation does not
reopen an excluded parent; keep parents permitted, as above, or explicitly
reinclude the required directories. Nested rule files
apply only in their subtree. New batches capture edited rules while existing
captures retain the rules admitted for their operation.

## Rust IgnoreScope contract

`crates/graphit-core/src/ignore_rules.rs` implements captured ignore scopes
with the mature `ignore` crate. AST selection and watchers use these scopes.

`IgnoreScope::load(root, start_relative, custom_filename, defaults)` loads rules
from the canonical project root to the starting scope. `child(basename)` enters
one existing child directory; `is_ignored(project_relative, is_dir)` tests a
path, and `should_descend(project_relative_directory)` tells a future walker
whether to enter it. Getters expose the root and scope directory. Consumers
supply their defaults, including configured documentation exclusions; this
helper does not duplicate AST/Knowledge's default lists.

The first source with a match decides, in this priority order:

1. Project defaults.
2. Custom ignore files, from the deepest loaded scope to the root.
3. `.gitignore` files, from the deepest loaded scope to the root.

Rules apply relative to their source directory and do not leak into siblings.
Local `.gitignore` files are read even without a `.git` directory; global Git
rules, `.git/info/exclude` and remote sources are not read. Within each source,
the last rule matching the queried path directly wins. If none matches directly,
the nearest matching parent is inherited using the crate's
`matched_path_or_any_parents` API. Each parent is resolved through the same
matcher before a descendant can be included. Both Git and custom files follow
the [Git parent-exclusion rule](https://git-scm.com/docs/gitignore): a file
negation cannot reopen an excluded directory. This includes scoped and
deleted-path queries; they need no filesystem lookup to resolve these prefixes.
For example, keep the directory permitted and exclude its children before
reincluding one file:

```gitignore
generated/*
!generated/driver.rs
```

By contrast, reversing `generated/driver.rs` and `!generated/driver.rs` changes
the result because both match that file directly. A custom negation cannot
override a matching default.

An unrelated negation such as `!.env.example` does not reopen `target/` or any
other excluded directory. No global whitelist state broadens traversal. The
helper uses the existing matcher for each native path prefix, without predicting
glob prefixes or scanning for rules below pruned directories. Walkers apply
`should_descend` before entering and check each child individually. Protected
defaults remain authoritative over both rule-file families.

Scopes share immutable `Arc` policy/layers and are `Clone`/`Send`/`Sync`. A fresh
load observes edits; existing clones retain loaded rules. Entering a child loads
its own layer then, so the tree is not an atomic filesystem snapshot. Queries
are lexical, perform no I/O, and can test deleted paths. They reject absolute,
traversing or out-of-scope paths; custom filenames and child names must be single
basenames. Child directory symlinks are rejected; a root alias is canonicalized.

Only observed regular rule files are accepted. Symlinks, including local,
external and dangling links, and nonregular sources return `InvalidInput`.
Actual absence supplies empty rules; other I/O errors propagate. Files are
limited to 256 KiB and UTF-8 with an optional initial BOM. Any parsing/build
error aborts loading rather than accepting a partial rule set. This validation
does not promise atomic protection against concurrent filesystem replacement
or timed interruption of filesystem I/O.

## Default Patterns

Defaults protect generated/private state; project custom rules do not reopen
a path rejected by those defaults.

### `.astignore`

The AST module has three built-in defaults; everything else is controlled by the `.astignore` file and supported file extensions.

| Pattern | Why | Adjustable |
|---|---|---|
| `.git/` | Git metadata directories contain mutable refs, locks and objects rather than project source. Automatic AST discovery and watch exclude them, including nested repositories. Custom negations cannot reopen them. This default does not change an explicitly authorized Source FILE read. | No |
| `.graphit/` | The project's Graphit directory contains identity, configuration, grammar query YAMLs and rule overrides. These control indexing rather than being parsed as project code. This exclusion applies independently of repository Git rules. | No |
| `knowledge.docs_dir` (default `docs/`) | The documentation tree belongs to the knowledge wiki. | Yes — `ast.index_docs` |

> **Markdown is excluded by the language set, not by a pattern.** No shipped query
> file claims `.md`, `.markdown` or `.mdx`, and extensions are what a query file
> grants, so a document is never discovered — no ignore pattern is consulted and
> `ast.index_docs` does not bring it back. The grammar itself is still compiled in,
> so a project that wants markdown structure opts in with its own `markdown.yaml`
> under `ast.queries_dir`. See
> [AST Module](../specs/ast_module.md#supported-languages).

> **Ignored by the indexer is not the same as ignored by git.** The `.gitignore`
> rules maintained by the repository select generated state and parser shared
> libraries (`.so`, `.dylib`, `.dll`). Authored grammar sources, project query YAMLs
> under `.graphit/ast/queries/` and rule overrides remain eligible for Git tracking.
> `graphit setup` does not write an automatic ignore block. See
> [Git Module](../specs/git_module.md) and
> [Storage Layout](../architecture/storage_layout.md#what-stays-in-the-project).
> The `.astignore` default above still keeps the **whole** brand directory out of the
> code graph, grammar YAMLs included, and that is intended: they configure the parser
> rather than being parsed. The two layers answer different questions, and a `!` line
> in `.astignore` cannot re-include what a built-in default excludes in the
> captured matcher contract.

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
graphit config set ast.index_docs true
```

This puts the *directory* back, not markdown: no shipped query file claims `.md`,
so what returns is the structured files under `docs/`. Markdown is a separate
opt-in — a `markdown.yaml` under `ast.queries_dir`.

**Use the key, not `.astignore`.** The exclusion is a built-in default, and
defaults outrank the project's own patterns in the captured matcher contract, so `!docs/` in `.astignore` has no
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
graphit config set ast.grammars_blacklist yaml

# out of every project on this machine
graphit config --global set ast.grammars_blacklist yaml

# or the other way round: index nothing but these
graphit config set ast.grammars_whitelist go,sql
```

Both keys are comma-separated. A name matches the language, the grammar, or the
grammar without its `tree-sitter-` / `antlr-` prefix — so `yaml` and
`tree-sitter-yaml` both name the same language. When the whitelist is non-empty it
is exhaustive, and the blacklist still subtracts from it. Semantics and precedence:
[config_module](../specs/config_module.md#turning-grammars-off-astgrammars_blacklist-and-astgrammars_whitelist);
enforcement and what happens to nodes already indexed:
[ast_module](../specs/ast_module.md#configuration-and-customization).

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
- **Agent config**: `.idea/`, `.vscode/`, `.vs/`
- **Agent config**: `.agents/`, `.claude/`, `.cursor/`, `.gemini/`, etc.
- **Graphit internal**: `.graphit/`

In the captured matcher contract, defaults outrank project
patterns, so a `!` negation in `.wikiignore` will not
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

- **Knowledge sync**: The next `graphit sync` enumerates the current source set, recomputes the
  cross-reference graph and communities, deletes ignored slugs from LanceDB, and upserts only rows
  whose compiled value changed.

AST shard directories are cleaned up automatically after file removal. Knowledge has no shard
directory.

## Rust AST policy from captured configuration

`graphit_ast::ignore_policy::AstIgnorePolicy::from_config` derives the module's
`.astignore` filename and protected defaults from the actual Core ConfigSnapshot
and Original. The defaults protect Git metadata directories and use the captured Brand directory, its root-only
`<brand>.lock.json`, and the root-only docs tree when `ast.index_docs=false`.
The Core matcher continues to own custom/Git rules, precedence and reinclusion;
this producer adds no walker or language filter.

The policy preserves admitted lexical docs path aliases, omits the docs exclusion
for the project root or paths escaping it, and quotes literal glob names with the
mature SDK. Winning configuration type errors remain errors. `index_docs=true`
removes the docs default without consulting the unused docs setting. For API,
path/error/native-platform details and qualification scope, see
[Policy Rust de ignores AST](../specs/rust_ast_ignore_policy.md).
The actual Main/watcher consumer remains a separate integration delivery.
