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

Ignore files can be placed at any level of the directory hierarchy, just like `.gitignore`:

- **Project root**: Rules apply to all files
- **Subdirectory**: Rules apply relative to that directory (scoped)

Files are collected upward from the project root to the git root, allowing nested overrides.

## Default Patterns

### `.astignore`

The AST module has no built-in default patterns. All filtering is controlled by the `.astignore` file and supported file extensions.

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

These defaults can be overridden with `!` negation patterns in `.wikiignore`.

## Effect of Changes

When an ignore file is modified:

- **AST sync**: The next `graphit sync` or daemon cycle detects changed files. Files that are now ignored are removed from the graph database and shard cache (including `.nodes.json`, `.edges.json`, and `.emb.json` files). Files that are now included are parsed and added.

- **Knowledge sync**: The next `graphit sync` detects the ignore file mtime change (tracked via `StatPreCheck` watch files), forces a full walk, prunes removed files from the process cache, removes their wiki pages, and rebuilds the cross-reference graph, backlinks, communities, and FTS5 database.

In both cases, empty shard directories are cleaned up automatically after file removal.
