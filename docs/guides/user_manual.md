---
title: "User Manual"
description: "User guide on navigating the 3D dashboard, managing memories, deploying rule templates, and utilizing knowledge wikis."
content-type: guide
audience: developers
keywords:
  - user manual
  - dashboard
  - 3D graph
  - memory
  - wiki
  - dream
  - idle
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/cli_reference.md"
  - "docs/architecture/architecture_overview.md"
---

# User Manual

This manual explains how to interact with the Graphit Code system.
It covers how to use the visual dashboard, manage the memory database, set up autonomous skill generation, and work within the docs-as-code collaborative flow.

---

## Navigating the Visual Dashboard

To open the unified web application, run:
```bash
graphit ui
```
This launches a browser window (default: `http://localhost:8080`).
The interface contains a sidebar with three main modules:

### 1. Abstract Syntax Tree (AST) Explorer
The AST Explorer features an interactive **3D force-directed node canvas** that visualizes your codebase:
- **3D Canvas Navigation**: Drag with your mouse to rotate the graph, scroll to zoom, and right-click to pan.
- **Node Selections**: Click on a node (representing a function, file, or class) to highlight its connections. The sidebar displays its properties (e.g., source file path, cycle complexity, and docstring).
- **Cypher Queries**: Execute custom Cypher queries in the query bar. For example:
  ```cypher
  MATCH (fn:Function) WHERE fn.cyclomatic_complexity > 10 RETURN fn.name, fn.path
  ```
  The canvas renders the matching subset of nodes, while the results panel lists data in a tabular format.

### 2. Wiki and Knowledge Explorer
The Wiki Explorer indexes documentation and structured files in your codebase:
- **Default Indexing Path**: By default it scans `docs/` plus the project root's `README.md`, respecting ignore rules. Point it somewhere else with `knowledge.docs_dir`, set that key to `.` to scan the whole project as earlier versions did, or set `knowledge.include_readme=false` to index the docs tree alone. See [ignore_files](ignore_files.md) for what reaches the wiki before ignore rules apply.
- **Configurable Extensions**: The set of file extensions to index is configurable via `knowledge.extensions`. By default, it indexes 16 extensions: `.md`, `.markdown`, `.mdx`, `.txt`, `.adoc`, `.rst`, `.puml`, `.plantuml`, `.yaml`, `.yml`, `.json`, `.proto`, `.graphql`, `.gql`, `.wsdl`, `.xml`.
- **Multi-Format Rendering**: Markdown files (`.md`, `.markdown`, `.mdx`) are split by H2 headers into parent/child pages and rendered as native markdown. Structured data files (`.yaml`, `.json`, `.graphql`, `.xml`) are rendered as syntax-highlighted code blocks. Other formats (`.proto`, `.rst`, `.txt`, etc.) are rendered as plain monospaced text.
- **Collapsed Tree**: The sidebar tree starts collapsed. Nodes expand automatically when their children are selected, keeping the navigation clean.
- **Index & Logs**: Read a compiled list of all registered documents, categorized into community graphs.
- **Staleness Tracking**: Pages whose source files have changed since the last sync are flagged as stale, with transitive propagation to dependent pages.
- **Community Clusters**: Documents are grouped into thematic communities using Louvain graph clustering on cross-reference edges.
- **Search**: Perform unified searches combining Full-Text Search (FTS) and semantic keyword matching.
- **Wikilinks**: Click on highlighted links to explore adjacent topics and track inbound back-references.

#### Customizing Extensions

Set the extensions globally (all projects) or per project:

```bash
# Global — applies to all projects
graphit config knowledge.extensions "md,yaml,json,proto,graphql"

# Per project — in .graphit/config.json or graphit.lock.json config section
{
  "knowledge": {
    "extensions": "md,txt,yaml,yml,json,rst"
  }
}
```

The `.` prefix is optional — `md` and `.md` are equivalent. If not configured, all 16 default extensions are used.

Environment variable override: `GRAPHIT_KNOWLEDGE_EXTENSIONS="md,yaml,json"`

### 3. Collaboration Hub Manager
The Hub Manager allows you to review rules and agent configurations shared across the team:
- **Registry View**: Inspect available plugins, skills, and MCP tools in the registry.
- **Installs**: Click to deploy templates or commands into your local project structure.

#### Hub Artifact Types

The Hub supports 10 artifact types. One is dedicated to the AST module's language pipeline:

- **Language Queries** (`language`): Packages extraction `.yaml` query files that customize how entities are extracted from the built-in languages. These can override default extraction patterns, export strategies, and language configuration. On installation, the `.yaml` queries are placed into `<project>/.graphit/ast/queries/`.

Install Hub artifacts using the CLI:
```bash
# Install a language grammar (e.g., Elixir support)
graphit hub install elixir-lang@1.0
```

After installation, run `graphit sync` to activate the new language. No recompilation is required.

---

## Managing the Memory Lifecycle

AI agents often suffer from "session amnesia"—forgetting your preferences, style guidelines, and corrections as soon as a conversation ends.
Graphit Code solves this by dividing memory into two scopes:
- **Project Memory**: Shared through the configured Hub bucket's project-memory prefix. Best for database architectures, API contracts, and design conventions.
- **User Memory**: Local-only when no bucket is configured, or published to the bucket's user-memory prefix. Best for personal coding preferences.

Neither is stored inside the project. Both live once in the global directory — the raw
markdown in `~/.graphit/memory-wt/`, the compiled wiki in `~/.graphit/wiki/memory/` —
and are read from there by every project, which is why a memory you record in one
project is not duplicated into the next. See
[Storage Layout](../architecture/storage_layout.md).

### Memory Categories

Memory cards are structured around four types:
1. **Conventions**: Rules or patterns the agent must conform to (e.g., "Use HSL tailored colors for UI components").
2. **Corrections**: Important directives logged when the agent makes a mistake (e.g., "Do not call OS stdout directly, use output package printer").
3. **Decisions**: Architectural decisions or ADRs explaining why the system was built a certain way.
4. **Skills**: Reusable discoveries or scripts to solve recurrent debugging challenges.

### Modifying Memories via CLI
You can insert, delete, or list memories directly using the CLI:
```bash
# Add a new convention memory
graphit memory insert --title "API_Response_Format" --type "convention" --content "All API endpoints must wrap responses in a standard JSON metadata envelope."

# List active memories
graphit memory list

# Delete a memory
graphit memory delete --title "API_Response_Format"
```

---

## Autonomous Skill Generation (Dreaming)

The Dream module allows AI agents to mine conversation history and generate reusable knowledge autonomously when you are away:
1. **Preconditions**: The daemon process must be running (`graphit daemon`).
2. **Idle Inactivity**: The system monitors file changes. If no modifications occur within the idle timeout (default: 2 hours), a dream session is triggered.
3. **Conversation Mining**: The agent reviews past conversation logs to identify recurring patterns, corrections, and undocumented conventions.
4. **Skill Generation**: Identified patterns are crystallized into reusable skills, rules, and commands.
5. **Skill Effectiveness Evaluation**: Existing skills are analyzed for failures and improved using a self-healing loop with root cause classification.
6. **Integration Skills**: The module creates skills designed for external developers integrating with your project.

### Recording Future Work — the Task Backlog
The **task backlog** records work that should survive the conversation or session that identified it. It is a task registry, separate from Dream:
```bash
# Record a task
graphit backlog add "Create skill for deployment workflow" --body "Review conversations about deployment to extract a reusable skill"

# Check recorded tasks
graphit backlog list

# Drop an item you have since handled yourself
graphit backlog rm create-skill-for-deployment-workflow
```
Items are Markdown files under `backlog.dir` — `docs/tasks/backlog/` by default — so they are committed with the project and visible in review. Dream never consumes these items; it improves project knowledge instead. Full details are in [Task Backlog](../specs/backlog.md).

> Adding, listing, and removing items always works. `modules.dream` and daemon state are unrelated to backlog operations.

### Reviewing Dream Reports
After the session finishes, it produces a markdown report detailing the skill generation findings, conversation analysis results, and any new memories or skills created:
```bash
# List recent dream reports
graphit dream reports

# Read a report
cat .graphit/runtime/dream/<session-id>.md
```

The runtime vault is intentionally ignored. To publish reports, set
`dream.reports_dir` to a versioned directory such as `docs/dream`. Existing reports in
the former `.graphit/dream/` location are not moved or deleted automatically; point
`dream.reports_dir` there temporarily if you still need to list them.

---

## Customizing AST Tree-sitter Queries

The AST module extracts code entities (functions, classes, imports, etc.) from source files using **Tree-sitter query patterns**. These patterns are defined in **YAML files** that you can fully customize — adding new extraction patterns, removing defaults, or replacing the entire query set for a language — all without recompiling.

### How It Works

When Graphit Code parses a source file, it resolves query patterns using a **3-level priority chain** (all YAML — there is no hardcoded Go fallback):

1. **Project** (`ast.queries_dir`, `.graphit/ast/queries/` by default) — Highest priority. Applies only to this project.
2. **User Global** (`~/.graphit/ast/queries/`) — Your personal customizations. Applies to all projects. **Never written by the framework.**
3. **Runtime** (`~/.graphit/runtime/<version>/ast/queries/`) — Factory defaults extracted by the launcher during binary setup. **Automatically updated on each version upgrade.**

> The runtime defaults serve as the base. They are automatically extracted by the launcher during binary setup and updated on each version upgrade. YAML extraction rules and language configuration follow the 3-level resolution chain — query customization requires no recompilation.

For each language, the **first source that provides queries wins**. If you create a `go.yaml` in your project, only Go queries use the project version — all other languages continue resolving from user → runtime.

That winning file **replaces** the level below it, unless it declares `merge: true` at its root — then it merges instead, and you write only the part you are changing. See [Merging instead of replacing](#merging-instead-of-replacing-merge-true).

> **A query file can also add a language, not just adjust one.** A grammar is
> compiled into the binary; a query file is what binds extensions to it. Markdown is
> the case that exists on purpose: `tree-sitter-markdown` ships, but no runtime
> `markdown.yaml` does, so `.md` files are not indexed and documents stay the
> knowledge wiki's. Writing your own `markdown.yaml` into `ast.queries_dir` — with
> `language: markdown`, `grammar: tree-sitter-markdown` and the extensions you want —
> puts markdown structure in the code graph for that project alone.

### Viewing the Defaults

After your first `graphit sync` or `graphit ast index`, the runtime defaults are extracted to:
```
~/.graphit/runtime/<version>/ast/queries/
```

Browse these files to see every Tree-sitter pattern used for each language:
```bash
ls ~/.graphit/runtime/*/ast/queries/
# c.yaml  cpp.yaml  csharp.yaml  dart.yaml  go.yaml  java.yaml
# javascript.yaml  kotlin.yaml  php.yaml  plsql.yaml  postgresql.yaml
# python.yaml  ruby.yaml  rust.yaml  sql.yaml  swift.yaml
# tsql.yaml  db2.yaml  tsx.yaml  typescript.yaml  xml.yaml  html.yaml
```

### Customizing Globally (All Projects)

To modify queries for all your projects, copy the default file to the user global directory and edit it:

```bash
# Create the user global directory
mkdir -p ~/.graphit/ast/queries/

# Copy the runtime default as a starting point
cp ~/.graphit/runtime/*/ast/queries/go.yaml ~/.graphit/ast/queries/go.yaml

# Edit to add your custom patterns
$EDITOR ~/.graphit/ast/queries/go.yaml
```

### Customizing Per Project

A project's grammar directory is `ast.queries_dir`, relative to the project root. It
defaults to `.graphit/ast/queries/`, and **that default is tracked by git**. The generated
ignore block covers `.graphit/runtime/` and the platform-specific parser binaries in
`.graphit/grammars/`, but not query YAMLs. Commit the YAML and every other checkout gets
the same query override.

```bash
mkdir -p .graphit/ast/queries
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml
$EDITOR .graphit/ast/queries/python.yaml
git add .graphit/ast/queries/python.yaml
```

**Point the key somewhere else if you would rather keep grammars beside your other
tooling:**

```bash
graphit config ast.queries_dir tooling/grammars

mkdir -p tooling/grammars
cp ~/.graphit/runtime/*/ast/queries/python.yaml tooling/grammars/python.yaml
$EDITOR tooling/grammars/python.yaml
```

The configured directory replaces the default rather than adding to it: files left
under `.graphit/ast/queries/` are not read once the key is set. The change lands on a
running daemon within seconds — no restart.

### Turning a Language Off

Customizing a query file changes *how* a language is parsed. Two keys decide
*whether* it is parsed, and both are comma-separated lists:

```bash
# stop indexing YAML here
graphit config ast.grammars_blacklist yaml

# …and on every project on this machine
graphit config --global ast.grammars_blacklist yaml

# the other way round: index nothing but Go and SQL
graphit config ast.grammars_whitelist go,sql

# one command only
GRAPHIT_AST_GRAMMARS_BLACKLIST=yaml graphit ast index
```

`ast.grammars_whitelist` is exhaustive when it is not empty: everything it does not
name is off. `ast.grammars_blacklist` subtracts on top of it, so a grammar in both
lists is disabled.

A name matches the language, the grammar, or the grammar without its
`tree-sitter-` / `antlr-` prefix — `yaml`, `yaml_lang` and `tree-sitter-yaml` are the
same language, and `antlr-plsql` names one SQL dialect without touching the others.
A name matching nothing is inert: no error, and nothing
disabled — so check the spelling with `graphit config --list` if a language you
expected to drop is still in the graph.

Disabling takes files out of discovery, so the next **full** `graphit ast index`
also removes what was already indexed. A scoped run (`--path`) cannot: it never
walks the tree, so it has nothing to compare against.

> This is the third and broadest of the three exclusion axes — language, extension,
> path. See [Ignore Files](ignore_files.md#excluding-a-language-rather-than-a-path)
> for how they compose, and
> [config_module](../specs/config_module.md#turning-grammars-off-astgrammars_blacklist-and-astgrammars_whitelist)
> for the precedence chain.

One thing to expect if you do move it: a directory in the ordinary source tree is
indexed as code, where the whole of `.graphit/` is not, so those YAMLs will show up in
the code graph. Add the directory to `.astignore` if you would rather they did not.

### Merging instead of replacing (`merge: true`)

By default the winning file **is** the language: everything the level below said is
gone. So overriding one pattern meant copying the whole shipped file — and then owning
every future fix to it. Worse, the copy silently takes over `extensions` and `grammar`
too, and a copy that omits them breaks the language instead of leaving it alone.

Declare `merge: true` at the root and the file merges onto the level below instead.
It works at every level — yours over the runtime's, the project's over both. What pairs
your file with the one below is the **`language` field alone** (case does not matter);
`extensions` is one of the things you inherit, not part of the pairing:

```yaml
# tooling/grammars/python.yaml — three lines instead of three hundred
language: python
merge: true
queries:
  - data_key: decorators
    graph_label: Annotation
    pattern: '(decorator (identifier) @name)'
```

What that inherits, and what it replaces:

| Field | With `merge: true` |
|---|---|
| `extensions`, `parser`, `grammar`, `start_rule` | inherited when you omit them — this is what makes a three-line file a working language |
| `queries` | merged by `data_key`: redeclaring a key replaces that group, a new key is added, everything else is inherited |
| `context_types`, `context_name_paths`, `text_normalizers` | merged key by key; add one entry without restating forty |
| `self_keywords`, `declaration_types`, `comment_types`, `anon_func_types`, `exports` | replaced when you declare them, inherited when you don't — declaring a shorter list is how you remove an entry |
| `embedded` | your blocks go first, then the ones below; the first block that matches a body claims it |
| `complexity` | `node_types`, `operators` and `head_calls` each replaced-if-declared |

Leave the flag out and nothing changes from how it has always worked: your file is the
whole language. And if the language you name is one no lower level declares, there is
nothing to merge onto — the file simply stands on its own, which is how you introduce a
new language either way.

The combination this was written for is a project that adds an embedded block plus the
normalizer it needs — see [Where to declare it](#where-to-declare-it). Both merge, so
the result is the shipped language plus your dialect, with every query it already had.

### Example: Adding Custom Patterns

To track goroutines as function entities in Go, add a new query entry to `go.yaml`:

```yaml
language: go
extensions: [".go"]
queries:
  # ... keep existing queries ...

  # Custom: track goroutine launches
  - data_key: goroutines
    graph_label: Function
    pattern: '(go_statement (call_expression function: (identifier) @fn))'
    name_capture: fn
```

### Example: Completely Replacing Queries

Replacing is the **default** — no flag needed. A file for a language is that language, and
the levels below it are ignored:

```yaml
language: sql
extensions: [".sql"]
queries:
  - data_key: tables
    graph_label: Table
    pattern: '(create_table_statement name: (identifier) @name)'
  - data_key: procedures
    graph_label: Function
    pattern: '(create_procedure_statement name: (identifier) @name)'
```

Everything the runtime's `sql.yaml` declared is gone here — its queries, and also its
`extensions`, `grammar`, `context_types` and the rest, so anything this file needs it must
restate. That is what `merge: true` exists to avoid; see
[Merging instead of replacing](#merging-instead-of-replacing-merge-true).

### YAML Reference — Query Files

| Field | Required | Description |
|---|---|---|
| `language` | ✅ | Tree-sitter language name (e.g., `go`, `python`, `typescript`) |
| `extensions` | ❌ | File extensions filter (e.g., `[".ts"]`). Omit to match all extensions |
| `merge` | ❌ | `true` = merge into the same language at the level below, field by field. Omitted (the default) = replace it entirely. Files pair on `language` alone, case-insensitively |
| `queries[].data_key` | ✅ | Entity category: `functions`, `classes`, `imports`, `calls`, `fields`, etc. |
| `queries[].type` | ❌ | `"entity"` (default) or `"relation"`. Entities become graph nodes; relations become edges. |
| `queries[].relation_type` | ❌* | Required when `type: relation`. Edge label in the graph: `CALLS`, `INSTANTIATES`, `INHERITS`, `IMPLEMENTS`, `READS_FIELD`, `WRITES_FIELD`, `DECORATOR`, `EXPORT`, or any custom string. |
| `queries[].graph_label` | ❌ | LadybugDB node label (e.g., `Function`, `Class`). Empty = relational data |
| `queries[].pattern` | ✅ | Tree-sitter S-expression query |
| `queries[].name_capture` | ❌ | Capture group name for the entity. Default: `name`. Only this capture becomes an entity — every other one exists for a predicate to test, which is what the `@_` prefix convention signals |
| `queries[].value_capture` | ❌ | The value the entity is set to — for key/value languages, where the key alone is half the content. It becomes a node named after itself (so search reaches it), contained by the key, and is also written to the key's `value` property. Requires `value_label`; ignored on `type: relation` |
| `queries[].value_label` | ✅* | Required with `value_capture`. Node label for the value (e.g. `AttributeValue`, `Value`) |
| `queries[].parent_capture` | ❌ | Capture holding the name of the entity that contains this one, producing a `CONTAINS` edge. Use it when `context_types` cannot resolve the parent — tree-sitter-xml `element`, tree-sitter-json `pair` and tree-sitter-html `start_tag` have no `name` field for the tree walk to read. Requires `parent_label` |
| `queries[].parent_label` | ✅* | Required with `parent_capture`. Node label of the containing entity |
| `exports` | ❌ | Export detection config (see [Language Configuration](#customizing-language-configuration)) |
| `exports.strategy` | ✅* | One of: `capitalized_name`, `no_prefix`, `modifier`, `export_statement`, `no_modifier`, `no_static`, `none` |
| `exports.config` | ❌ | Key-value config for the strategy (e.g., `prefix: "_"`) |
| `exports.config_list` | ❌ | List-type config values (e.g., `keywords: [private, protected]`) |
| `self_keywords` | ❌ | Self/this keywords for receiver type resolution (e.g., `["self.", "this."]`) |
| `context_types` | ❌ | Map of Tree-sitter node types to graph labels (e.g., `class_definition: Class`) |
| `context_name_paths` | ❌ | How to read a context node's name when it is not in a `name` field: a `/`-separated path of field names or child kinds, optionally indexed (`string_lit[1]`). Data-format and markup grammars need this — see [Context Types](#context-types) |
| `anon_func_types` | ❌ | Tree-sitter node types for anonymous function detection |
| `declaration_types` | ❌ | Node types eligible for docstring attachment |
| `comment_types` | ❌ | Node types recognized as comments for docstring extraction |
| `embedded` | ❌ | Regions of the file written in **another language** — the body of a single-file component's `<script>` / `<style>`. See [Embedded Languages](#embedded-languages) |
| `embedded[].pattern` | ✅* | Tree-sitter query that selects the blocks — the same query language as `queries[].pattern`, so `#eq?`, `#match?` (regex), sibling anchors and nesting all work |
| `embedded[].text_capture` | ✅* | Capture in `pattern` whose node's text **is** the body |
| `embedded[].lang_capture` | ❌ | Capture holding the value that selects the language. Omit to always use `default` |
| `embedded[].default` | ❌* | Language used when `lang_capture` is absent or resolves empty |
| `embedded[].languages` | ❌* | Map of captured value → language name. An **allowlist**: a value that is not a key is skipped in silence. An explicit `{}` means "claim these bodies and map none of them" |
| `embedded[].normalize` | ❌ | Name of one of this language's `text_normalizers` to run on the body before the sub-parse. Needed whenever the host escapes its text — an XML element's content does, an HTML `<script>`'s raw text does not |
| `text_normalizers` | ❌ | Named ways this language turns escaped text back into what it represents, for an `embedded` block to name. Each entry: `replace` (literal → replacement) and `numeric_char_refs` (decode `&#62;` / `&#x3E;`). **A replacement containing a line break is dropped at load time**, because changing the newline count would shift every line the sub-parse reports. See [Normalizing an escaped body](#normalizing-an-escaped-body) |
| `complexity` | ❌ | What counts as a decision point when scoring `cyclomatic_complexity`, by walking the entity's own parsed subtree. `node_types` = named kinds (`if_statement`, a case clause), `operators` = anonymous tokens (`"&&"`, `"\|\|"`), `head_calls` = for grammars where every control form is the same node kind, told apart by the text of its first child (Clojure, Elixir). **Omitted means complexity stays at the base 1** for that language. See `docs/specs/ast_module.md` |

> For the full technical specification and implementation details, see `docs/specs/ast_module.md`.

---

## Customizing Language Configuration

Beyond Tree-sitter queries, each language YAML file can also define language-level configuration that controls how the AST engine resolves exports, self references, parent contexts, and docstrings.

### Export Strategies

The `exports` section defines how the engine determines whether a function, class, or variable is exported (public). There are six strategies:

| Strategy | Description | Example Languages |
|---|---|---|
| `capitalized_name` | Exported if the name starts with an uppercase letter | Go |
| `no_prefix` | Exported if the name does **not** start with a given prefix | Python (`_` prefix = private) |
| `modifier` | Exported if a visibility modifier keyword is present | Java, C# (`public`) |
| `export_statement` | Exported if referenced in an `export` statement | JavaScript, TypeScript |
| `no_modifier` | Exported if **none** of the private modifier keywords are present | Ruby (`private`, `protected`) |
| `no_static` | Exported if the function is not static | Swift |
| `none` | No export detection (all entities treated equally) | C, SQL |

Example configuration:
```yaml
# Python: exported if name does NOT start with "_"
exports:
  strategy: no_prefix
  config:
    prefix: "_"

# Java: exported if "public" modifier is present
exports:
  strategy: modifier
  config:
    keyword: "public"
```

### Self Keywords

The `self_keywords` list defines the keywords used for receiver type resolution (tracking method calls through `self.method()` or `this.method()`):

```yaml
self_keywords: ["self."]          # Python
self_keywords: ["this.", "this->"] # Java, C++
```

### Context Types

The `context_types` map tells the engine how to resolve parent context for nested entities. It maps Tree-sitter node type names to graph labels:

```yaml
context_types:
  class_definition: Class
  function_definition: Function
  method_definition: Method
  struct_declaration: Struct
  interface_declaration: Interface
```

### Declaration Types and Comment Types

- **`declaration_types`** — Node types eligible for docstring attachment. When the engine encounters a comment immediately before a declaration node, it attaches the comment as a `docstring` property.
- **`comment_types`** — Node types recognized as comments for docstring extraction (e.g., `comment`, `block_comment`, `line_comment`).

### Embedded Languages

A single-file component is several languages in one file, and the outer grammar does not look inside: `tree-sitter-vue`, `tree-sitter-svelte` and `tree-sitter-html` all hand the body of `<script>` and `<style>` over as one opaque text node. The `embedded` list says which regions are written in another language, so each is parsed with **that** language's grammar and merged back at the file's absolute line numbers.

This is what makes an `import` inside a component produce a real `IMPORTS` edge.

**A block is selected by a tree-sitter query** — the same query language as `queries[].pattern`. There is one mechanism, and it is the same one whether the block is a Vue `<script>` or an `<execute>` element in your own XML: "which node is this block" is the same question, and only a query answers it in general.

```yaml
# vue.yaml — the `lang` attribute is OPTIONAL, so the block takes two patterns
embedded:
  # lang present: the pattern captures the value, lang_capture names it
  - pattern: '(script_element (start_tag (attribute (attribute_name) @_a (quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a "lang"))'
    text_capture: body
    lang_capture: lang
    languages:
      js: javascript
      ts: typescript
      tsx: tsx
  # lang absent: the fallback, behind the specific one
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: javascript
```

**Order is part of the mechanism.** The first block whose pattern matches a given body node **claims** it; later blocks skip that body. That is how two patterns express one optional attribute, with no special case in the engine.

The claim is taken **at the match**, before the language resolves. That is what makes `<style lang="scss">` skip rather than fall through:

```yaml
  - pattern: '(style_element (start_tag (attribute (attribute_name) @_a (quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a "lang"))'
    text_capture: body
    lang_capture: lang
    languages: {}          # every preprocessor: claim the body, map none of it
  - pattern: '(style_element (raw_text) @body)'
    text_capture: body
    default: css
```

**`languages` is an allowlist, not a list of exceptions.** A value that is not a key is skipped **in silence** — `<style lang="scss">` is ordinary Vue and has no grammar here. An explicit `languages: {}` is a real declaration, distinct from omitting the key: it means "match these bodies, claim them, and map none of their values". Without the claim, the generic block behind it would parse SCSS as CSS and report entities from a grammar that never saw the real syntax.

That property is also how HTML tells code from payload. `html.yaml` reads `type` rather than `lang` — only the pattern changes, not the mechanism — and maps only the values that *are* JavaScript, so `type="application/json"` and `type="importmap"` are skipped instead of parsed as code.

#### Selecting any node, in any grammar

Because the selector is a query, it reaches cases no fixed node kind could. To index the body of `<execute>` in your project's XML as SQL, drop a `xml.yaml` into `.graphit/ast/queries/` and add:

```yaml
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
    text_capture: body
    default: sql
```

`#match?` takes a regex, so `(#match? @_tag "^sql")` covers a family of tag names without listing them. Sibling anchors and nesting express context — "only inside `<mapper>`". And because the pattern also locates the language value, a grammar that spells attributes differently needs no engine change: tree-sitter-xml writes `Attribute` → `Name` / `AttValue`, and a pattern says so directly.

> **Add `merge: true`, or the rest of the XML extraction disappears.** Override is per language: a project `xml.yaml` containing only `embedded:` **replaces** the runtime's XML file, queries and all. Declaring `merge: true` at the root keeps them and adds your block in front — see [Merging instead of replacing](#merging-instead-of-replacing-merge-true). Without it, the only correct move is copying the whole file and editing it, as in [How to Override Per Project](#how-to-override-per-project).

#### Normalizing an escaped body

A block embedded in XML is **not plain text**, and this is the difference between working and silently wrong. `<` and `&` are markup, so a `WHERE qt > 0` reaches the file as `qt &gt; 0`, and the host grammar splits that content into `CharData` / `EntityRef` / `CharData`.

That has two consequences for the pattern:

1. Capture the **whole `content` node**, not a `CharData` child. A `CharData` capture takes only the first chunk, and the sub-parse gets a statement truncated at the first comparison operator — with no error anywhere.
2. Declare a **normalizer** so the captured body becomes readable again.

```yaml
text_normalizers:
  xml_entities:
    replace:
      "&lt;": "<"
      "&gt;": ">"
      "&amp;": "&"
      "&quot;": '"'
      "&apos;": "'"
    numeric_char_refs: true      # &#62; and &#x3E;

embedded:
  - pattern: '(element (STag (Name) @_t) (content) @body (#eq? @_t "execute"))'
    text_capture: body
    normalize: xml_entities
    default: plsql
```

**The engine knows no escaping scheme at all** — there is no entity table in the code and no "XML mode". How a language escapes its text is a fact about that language, so it is declared in YAML, exactly like `context_types`. A grammar that escapes differently declares its own.

**Declared by the language, chosen by the block**, because those are two different facts: an escaping scheme belongs to the language, but *needing* it belongs to the position. An XML element's content is escaped; an HTML `<script>`'s raw text is not, even though HTML has entities too.

> **A normalizer may not change the number of line breaks**, and the engine enforces it. Every line the sub-parse reports is shifted by the block's starting line in the host file, so a replacement that produced a newline would move every entity after it — turning a visible syntax error into a wrong line number. A pair whose replacement contains a line break is dropped at load time with a warning, and `&#10;` / `&#xA;` are left as written.

Anything not declared is left alone: a `&nbsp;` is not the engine's to guess at, and a bare `&` is far more likely to be an operator than a broken entity.

#### Where to declare it

Put the declaration where the need is. A **shipped** grammar should only declare a normalizer if the whole language needs one; a project that embeds SQL in its own XML dialect declares it in that project's own `xml.yaml` — under `ast.queries_dir`, `.graphit/ast/queries/` by default — and the resolution chain does the rest: no engine change, nothing added to the general grammar.

> A project file of two sections needs `merge: true`, since override is **per language**: without it, an `xml.yaml` carrying only `text_normalizers:` and `embedded:` replaces the runtime's XML file entirely and takes its queries with it. `text_normalizers` merges key by key and `embedded` puts your blocks first, so the merged file is the language plus your dialect.

#### Adding a language to a block

Mapping a new inner language is **one line of YAML, no rebuild**. To index the body of a JSON script block as real JSON:

```yaml
languages:
  module: javascript
  application/json: json        # ← this, and nothing else
```

The body then yields `json.yaml`'s own entities (`Pair`, `Value`) at the file's absolute lines, while the element's markup stays exactly as it was.

The only requirement is that the inner language **exists** — that it has a query file, which is what registers its extensions. Every shipped language qualifies, on **either backend**: `json`, `css`, `typescript`, `javascript`, `yaml`, `xml`, `python`, and the rest on tree-sitter, plus `plsql`, `postgresql`, `tsql`, `db2` and `cobol85` on ANTLR.

The config names the **language**; which backend parses it is the engine's problem. That matters for SQL in particular: the generic `sql` grammar knows DDL, but the dialect grammars are the ones that produce `SELECTS` / `INSERTS` / `UPDATES` edges and know what a stored procedure is. Naming `plsql` in an XML's `<execute>` block gives you those edges, out of the `.xml`, at absolute line numbers.

#### What is not configurable

One thing, and it fails safe: **nesting is one level deep.** A block inside a block inside a block is not parsed; no real component needs it, since `<script>` holds TypeScript and TypeScript declares no blocks.

Full design, the decisions behind it and the extensibility boundary: [Embedded Language Parsing](../specs/embedded_language_parsing.md).

### How to Override Per Project

To customize language configuration for a specific project, create or copy the language YAML file into the project's grammar directory (`ast.queries_dir`, `.graphit/ast/queries/` by default):

```bash
# Copy the default Python config as a starting point
mkdir -p .graphit/ast/queries/
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml

# Edit the export strategy, self keywords, etc.
$EDITOR .graphit/ast/queries/python.yaml
```

> Language configuration follows the same 3-level resolution chain as queries: project → user global → runtime (all YAML). The first source that provides configuration for a language wins — or merges into the level below it, if it declares `merge: true`.

---

## Adding New Language Support

Graphit Code supports 45 supported languages. Tree-sitter grammars (40 languages) are loaded dynamically via CGO dlopen, and 5 ANTLR grammars (PL/SQL, PostgreSQL, T-SQL, DB2, COBOL 85) run as sidecar binaries with IPC. **Adding an entirely new language grammar does not require recompilation — grammars are installed as plug-and-play binaries via the Hub.**

However, the YAML query files that control what gets extracted from the AST are fully customizable. You can:

- **Customize extraction queries** for any of the 45 supported languages
- **Override export strategies, self keywords, context types**, and other language configuration
- **Add or remove entity extraction patterns** per project or globally

### What You CAN Customize Without Recompilation

All extraction behavior is driven by YAML files that follow the 3-level resolution chain:

| Priority | Path | Scope |
|----------|------|-------|
| 1 | `ast.queries_dir` — `.graphit/ast/queries/` by default | Project-only |
| 2 | `~/.graphit/ast/queries/` | All projects (user) |
| 3 | `~/.graphit/runtime/<version>/ast/queries/` | Factory defaults |

To customize a built-in language, copy the runtime default and edit it:

```bash
# Copy the default as a starting point
mkdir -p .graphit/ast/queries/
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml

# Edit extraction patterns, exports, context types, etc.
$EDITOR .graphit/ast/queries/python.yaml
```

Or declare `merge: true` and write only what you are changing, instead of owning a
copy of the whole file — see [Merging instead of replacing](#merging-instead-of-replacing-merge-true).

### Adding a New Grammar (No Recompilation Required)

New grammars are installed as plug-and-play binaries via the Hub:

1. **Tree-sitter**: Install a shared library (`.so`/`.dylib`/`.dll`) via `graphit hub install <language>`. The grammar is extracted to `.graphit/grammars/treesitter/` and loaded dynamically via CGO `dlopen`.
2. **ANTLR**: Install a sidecar binary via `graphit hub install <language>`. The binary is extracted to `.graphit/grammars/antlr/` and launched as a subprocess with IPC.
3. **YAML queries**: Create or install the language YAML file defining extraction patterns. Drop into `.graphit/ast/queries/` or install via Hub.

No Go source changes or recompilation needed. The Makefile targets `make grammars-treesitter` and `make grammars-antlr` are used only for building new grammars from source.

### ANTLR Language Configuration

For languages parsed by ANTLR v4 (PL/SQL, PostgreSQL, T-SQL, DB2, COBOL 85), the YAML configuration uses XPath expressions instead of Tree-sitter S-expressions. ANTLR grammars run as sidecar binaries with IPC — installed via the Hub as `.grammar` archives.

ANTLR language files require `parser: antlr4`, `start_rule:`, and `grammar:` fields. Patterns use XPath syntax to navigate the ANTLR parse tree:

```bash
mkdir -p .graphit/ast/queries/
$EDITOR .graphit/ast/queries/plsql.yaml
```

**4. Define queries using XPath patterns:**

ANTLR language files require `parser: antlr4`, `start_rule:`, and `grammar:` fields. Patterns use XPath syntax to navigate the ANTLR parse tree instead of S-expressions:

```yaml
language: plsql
parser: antlr4
start_rule: sql_script
grammar: antlr-plsql
extensions:
  - .sql
  - .pks
  - .pkb
queries:
  - data_key: functions
    graph_label: Function
    pattern: "//create_function_body"
    name_capture: "function_name"
  - data_key: procedures
    graph_label: Function
    pattern: "//create_procedure_body"
    name_capture: "procedure_name"
  - data_key: packages
    graph_label: Class
    pattern: "//create_package_body"
    name_capture: "package_name"
  - data_key: types
    graph_label: Type
    pattern: "//type_definition"
    name_capture: "type_name"

exports:
  strategy: none

self_keywords: []
```

**5. Run `graphit sync` to index:**

```bash
graphit sync
```

### ANTLR-Specific YAML Fields

| Field | Required | Description |
|---|---|---|
| `parser` | ✅ | Set to `antlr4` to use the ANTLR backend. Omit or set to `tree-sitter` for the default |
| `start_rule` | ✅* | ANTLR start rule name (e.g., `sql_script`, `compilationUnit`). Required when `parser: antlr4` |
| `grammar` | ✅* | Name of the ANTLR grammar (e.g., `antlr-plsql`). Required when `parser: antlr4` |
| `queries[].pattern` | ✅ | XPath expression navigating the ANTLR parse tree (e.g., `"//create_function_body"`) |

All other YAML fields (`extensions`, `exports`, `self_keywords`, `context_types`, etc.) work identically for both parser backends.

### Grammar Selection (`ast.grammar` / `--grammar`)

By default, Tree-sitter is used when a grammar exists for the file extension; ANTLR is used as fallback. When both engines support the same extension, Tree-sitter is tried first and ANTLR is used only if Tree-sitter fails or extracts nothing.

**The SQL dialects are the exception, and they are the reason this setting exists.** `plsql`, `postgresql`, `db2`, `tsql` and `plpgsql` are **exclusive** grammars: they claim no extensions of their own, so `.sql` is parsed by the tree-sitter `sql` grammar with no dialect fallback behind it, and the dialect-only extensions — `.pks`, `.pkb`, `.prc`, `.db2`, `.tsql`, `.pgsql` — are not indexed unless you ask for them. Which dialect a repository is written in is not something the indexer can guess, and guessing it wrong used to cost four full ANTLR parses per file.

Bind an extension to a grammar in configuration, which is what discovery reads:

```bash
# this repository's SQL is Oracle, and so are its package files
graphit config ast.grammar ".sql=antlr-plsql,.pks=antlr-plsql,.pkb=antlr-plsql"

# T-SQL instead
graphit config ast.grammar ".sql=antlr-tsql"
```

Or per command, with the `--grammar` flag:

```bash
graphit sync --grammar .sql=antlr-plsql
graphit ast index --grammar .sql=tree-sitter-sql
```

The grammar name determines the backend automatically: names starting with `antlr-` use ANTLR, others use tree-sitter. A bound extension is parsed by the named grammar and by nothing else — there is no fallback behind an explicit choice.

Prefer the configuration key over the flag for an extension that no other grammar claims. File discovery, the watcher and the daemon read the key and have no command line, so `--grammar .pks=antlr-plsql` on its own tells the parser what to do with `.pks` files that were never offered to it. A grammar disabled by `ast.grammars_blacklist` stays disabled either way.

### Important Notes

- **Built-in grammars**: All 40 Tree-sitter grammars and 5 ANTLR grammars (PL/SQL, PostgreSQL, T-SQL, DB2, COBOL 85) are loaded dynamically at runtime. YAML query files (extraction patterns, export strategies, language configuration) are customizable via the resolution chain. New grammars can be added via Hub without recompilation.
- **Pattern validation**: Invalid Tree-sitter patterns are detected at parse time and logged as warnings, while valid patterns proceed normally. Invalid XPath expressions in ANTLR queries are similarly logged.
- **Customizing existing languages**: For the 44 languages indexed by default, all extraction rules, export detection, context resolution, and docstring attachment are fully YAML-driven. Changing the YAML is sufficient — no rebuild needed.
- **Parser field**: If the `parser` field is omitted from a YAML file, Tree-sitter is assumed. Existing YAML files do not need modification.

---

## Customizing Module Rules and Skills

The on-demand IDE agent follows **rules** and **skills** defined per module (AST, Knowledge, Memory, Hub, and Improvements). The background Dream agent uses these artifacts as context for autonomous skill generation and knowledge mining.
Rules are the compact instructions injected into the global rules file (e.g., `AGENTS.md`). Skills are the detailed instruction files that agents read on-demand.
Graphit Code provides a **multi-layer override system** so you can customize both at different scopes.

### Override Hierarchy (highest to lowest priority)

1. **Project-Level** — `.graphit/rules/<module>.md` / `<module>_skill.md` in the project directory. Applies only to that project.
2. **Global CLI** — `~/.graphit/rules/<module>.md` / `<module>_skill.md`. Applies to all projects on your machine.
3. **Hub Rule Prefix** — `rules/<module>.md` / `rules/<module>_skill.md` in the configured Hub bucket. Applies to all team members automatically.
4. **Compiled-In Default** — Built into the Graphit Code binary.

The first source found wins. This means a project-level override always takes precedence, followed by the user's global override, then the team's Hub-distributed override, and finally the built-in default.

### Managing Rules via CLI

Modules that provide agent guidance expose a `rule` subcommand, including `graphit ast rule`,
`graphit knowledge rule`, `graphit memory rule`, and `graphit hub rule`.

### Embedding the Default with Placeholders

Custom rule and skill files support placeholders that embed the compiled-in default content, allowing you to **wrap** the defaults with additional instructions:

**For rules** — use `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}`:

```markdown
# My Custom Security Requirements
- All endpoints must enforce mTLS in production.

## Standard Analysis
{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}
```

**For skills** — use `{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}`:

```markdown
# Extra Skill Instructions
- Always check for race conditions in concurrent code.

## Standard Skill
{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}
```

The placeholders are replaced at runtime with the full default content. This lets you extend the defaults rather than completely replacing them.

### Team-Wide Rules and Skills via Hub

To enforce standards across your entire team, publish rule and/or skill files to the
`rules/` prefix of the configured Hub bucket. For example:

```
Hub bucket
└── rules/
    ├── improvements.md          # team-wide improvements rule override
    ├── ast.md                   # team-wide AST rule override
    └── memory_skill.md          # team-wide memory skill override
```

Every team member receives these overrides on `graphit sync` or `graphit update` through
the S3-backed Hub, without separately configuring each project.

> For the full technical specification, see `docs/specs/rule_override.md`.


## Managed Rules and Sentinel Blocks

When you initialize a project using `graphit init`, the CLI installs instructions inside your IDE config files (such as `.cursorrules`, `.codeagent`, or `AGENTS.md`).
To keep these instructions up-to-date without overwriting your custom rules, Graphit uses **Sentinel Blocks**:

```html
<!-- GRAPHIT MEMORY BLOCK -->
# 🧠 Memory Management
...
<!-- END GRAPHIT MEMORY BLOCK -->
```

> [!WARNING]
> Do not modify the text inside these sentinels manually.
> The framework automatically overwrites their content during `graphit sync` or `graphit update`. Put your custom rules outside these blocks.

---

## Docs-as-Code Synchronous Flow

To keep the AST database, memories, and wiki indexes in sync with your source code updates:
1. Make code modifications in your editor.
2. Run a sync:
   ```bash
   graphit sync &
   ```
   The background daemon will pick up files, reindex, compute embeddings, run memory GC, and recompile the local Obsidian index.
3. Your AI agent can immediately query the updated files without needing to reload its context window.
