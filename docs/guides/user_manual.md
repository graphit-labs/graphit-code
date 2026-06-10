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
- **Default Indexing Path**: By default, it scans the entire project root directory (respecting ignore rules). You can customize this by setting `knowledge.docs_dir` in your configuration to point to a specific directory (like `docs/`).
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
graphit config set knowledge.extensions "md,yaml,json,proto,graphql"

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

The Hub supports 10 artifact types. Two are dedicated to the AST module's language and framework detection pipeline:

- **Language Queries** (`language`): Packages extraction `.yaml` query files that customize how entities are extracted from the built-in languages. These can override default extraction patterns, export strategies, and language configuration. On installation, the `.yaml` queries are placed into `<project>/.graphit/ast/queries/`.

- **Framework Configs** (`framework`): Packages a `.yaml` framework detection file defining decorator, heritage, and import detection rules for a framework. On installation, the `.yaml` file is placed into `<project>/.graphit/ast/frameworks/`, where its rules merge with built-in defaults on the next sync.

Install Hub artifacts using the CLI:
```bash
# Install a language grammar (e.g., Elixir support)
graphit hub install elixir-lang@1.0

# Install a framework detection config (e.g., Phoenix)
graphit hub install phoenix-framework@1.0
```

After installation, run `graphit sync` to activate the new language or framework. No recompilation is required.

---

## Managing the Memory Lifecycle

AI agents often suffer from "session amnesia"—forgetting your preferences, style guidelines, and corrections as soon as a conversation ends.
Graphit Code solves this by dividing memory into two scopes:
- **Project Memory**: Stored under `.graphit/memory/project/`. Shared across the team using a central Git repository. Best for database architectures, API contracts, and design conventions.
- **User Memory**: Stored under `.graphit/memory/user/`. Kept local to the machine or private repo. Best for personal coding preferences.

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

### Submitting Dream Subjects
You can queue tasks or instructions for the background agent to work on during its next dream cycle:
```bash
# Register a subject for the next dream cycle
graphit dream subject add "Create skill for deployment workflow" --body "Review conversations about deployment to extract a reusable skill"

# Check subjects queue
graphit dream subject list
```

### Reviewing Dream Reports
After the session finishes, it produces a markdown report detailing the skill generation findings, conversation analysis results, and any new memories or skills created:
```bash
# List recent dream reports
graphit dream reports

# Read a report
cat .graphit/dream/<session-id>.md
```

---

## Customizing AST Tree-sitter Queries

The AST module extracts code entities (functions, classes, imports, etc.) from source files using **Tree-sitter query patterns**. These patterns are defined in **YAML files** that you can fully customize — adding new extraction patterns, removing defaults, or replacing the entire query set for a language — all without recompiling.

### How It Works

When Graphit Code parses a source file, it resolves query patterns using a **3-level priority chain** (all YAML — there is no hardcoded Go fallback):

1. **Project** (`.graphit/ast/queries/`) — Highest priority. Applies only to this project.
2. **User Global** (`~/.graphit/ast/queries/`) — Your personal customizations. Applies to all projects. **Never written by the framework.**
3. **Runtime** (`~/.graphit/runtime/<version>/ast/queries/`) — Factory defaults extracted by the launcher during binary setup. **Automatically updated on each version upgrade.**

> The runtime defaults serve as the base. They are automatically extracted by the launcher during binary setup and updated on each version upgrade. YAML extraction rules and language configuration follow the 3-level resolution chain — query customization requires no recompilation.

For each language, the **first source that provides queries wins**. If you create a `go.yaml` in your project, only Go queries use the project version — all other languages continue resolving from user → runtime.

### Viewing the Defaults

After your first `graphit sync` or `graphit ast index`, the runtime defaults are extracted to:
```
~/.graphit/runtime/<version>/ast/queries/
```

Browse these files to see every Tree-sitter pattern used for each language:
```bash
ls ~/.graphit/runtime/*/ast/queries/
# c.yaml  cpp.yaml  csharp.yaml  dart.yaml  go.yaml  java.yaml
# javascript.yaml  kotlin.yaml  php.yaml  python.yaml  ruby.yaml
# rust.yaml  sql.yaml  swift.yaml  tsx.yaml  typescript.yaml
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

To customize queries for a single project, create the file in the project's `.graphit/` directory:

```bash
mkdir -p .graphit/ast/queries/
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml
$EDITOR .graphit/ast/queries/python.yaml
```

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

Set `replace: true` to discard all lower-priority queries and use only your definitions:

```yaml
language: sql
extensions: [".sql"]
replace: true   # Ignore runtime defaults entirely
queries:
  - data_key: tables
    graph_label: Table
    pattern: '(create_table_statement name: (identifier) @name)'
  - data_key: procedures
    graph_label: Function
    pattern: '(create_procedure_statement name: (identifier) @name)'
```

### YAML Reference — Query Files

| Field | Required | Description |
|---|---|---|
| `language` | ✅ | Tree-sitter language name (e.g., `go`, `python`, `typescript`) |
| `extensions` | ❌ | File extensions filter (e.g., `[".ts"]`). Omit to match all extensions |
| `replace` | ❌ | `true` = replace lower-priority queries; `false` = append (default) |
| `queries[].data_key` | ✅ | Entity category: `functions`, `classes`, `imports`, `calls`, `fields`, etc. |
| `queries[].type` | ❌ | `"entity"` (default) or `"relation"`. Entities become graph nodes; relations become edges. |
| `queries[].relation_type` | ❌* | Required when `type: relation`. Edge label in the graph: `CALLS`, `INSTANTIATES`, `INHERITS`, `IMPLEMENTS`, `READS_FIELD`, `WRITES_FIELD`, `DECORATOR`, `EXPORT`, or any custom string. |
| `queries[].graph_label` | ❌ | LadybugDB node label (e.g., `Function`, `Class`). Empty = relational data |
| `queries[].pattern` | ✅ | Tree-sitter S-expression query |
| `queries[].name_capture` | ❌ | Capture group name for the entity. Default: `name` |
| `exports` | ❌ | Export detection config (see [Language Configuration](#customizing-language-configuration)) |
| `exports.strategy` | ✅* | One of: `capitalized_name`, `no_prefix`, `modifier`, `export_statement`, `no_modifier`, `no_static`, `none` |
| `exports.config` | ❌ | Key-value config for the strategy (e.g., `prefix: "_"`) |
| `exports.config_list` | ❌ | List-type config values (e.g., `keywords: [private, protected]`) |
| `self_keywords` | ❌ | Self/this keywords for receiver type resolution (e.g., `["self.", "this."]`) |
| `context_types` | ❌ | Map of Tree-sitter node types to graph labels (e.g., `class_definition: Class`) |
| `anon_func_types` | ❌ | Tree-sitter node types for anonymous function detection |
| `declaration_types` | ❌ | Node types eligible for docstring attachment |
| `comment_types` | ❌ | Node types recognized as comments for docstring extraction |

### YAML Reference — Framework Files

| Field | Required | Description |
|---|---|---|
| `framework` | ✅ | Framework identifier (e.g., `spring`, `flask`, `express`) |
| `languages` | ❌ | Languages this framework applies to (e.g., `[java, kotlin]`) |
| `decorator_detection[]` | ❌ | List of decorator names and categories to detect framework usage |
| `decorator_detection[].name` | ✅ | Decorator name to match (e.g., `RestController`) |
| `decorator_detection[].category` | ✅ | Framework category (e.g., `web`, `orm`, `di`, `test`) |
| `decorator_detection[].framework_name` | ❌ | Override the parent `framework` name for this rule |
| `heritage_detection[]` | ❌ | List of parent class/interface names to detect via inheritance |
| `heritage_detection[].parent` | ✅ | Parent class or interface name (e.g., `JpaRepository`) |
| `heritage_detection[].category` | ✅ | Framework category |
| `heritage_detection[].framework_name` | ❌ | Override the parent `framework` name |
| `import_detection[]` | ❌ | List of import path patterns to detect framework usage |
| `import_detection[].pattern` | ✅ | Import path pattern (e.g., `github.com/gin-gonic/gin`) |
| `import_detection[].match` | ❌ | Match strategy: `"prefix"` (default), `"exact"`, `"contains"`, `"suffix"`, or `"regex"` (Go `regexp` syntax) |
| `import_detection[].category` | ✅ | Framework category |
| `import_detection[].framework_name` | ❌ | Override the parent `framework` name |
| `entry_points` | ❌ | Entry point scoring rules (see [Entry Point Scoring](#customizing-entry-point-scoring)) |
| `entry_points.names[]` | ❌ | Name-based scoring rules using glob patterns |
| `entry_points.names[].pattern` | ✅ | Glob pattern: `main` (exact), `Test*` (prefix), `*Handler` (suffix), `*cmd*` (contains) |
| `entry_points.names[].score` | ✅ | Score to add when the pattern matches |
| `entry_points.decorators[]` | ❌ | Decorator-based scoring rules |
| `entry_points.decorators[].name` | ✅ | Decorator name to match |
| `entry_points.decorators[].score` | ✅ | Score to add when the decorator is present |
| `entry_points.exported_bonus` | ❌ | Bonus score for exported functions (default: `10`) |
| `entry_points.max_score` | ❌ | Maximum score cap (default: `100`) |

### YAML Reference — Ecosystems File

| Field | Required | Description |
|---|---|---|
| `config_files[]` | ✅ | List of ecosystem detection entries |
| `config_files[].filename` | ✅ | Config filename to detect (e.g., `package.json`, `Cargo.toml`) |
| `config_files[].language` | ✅ | Language associated with this config file |
| `config_files[].ecosystem` | ✅ | Ecosystem identifier (e.g., `node`, `cargo`, `django`) |
| `config_files[].glob` | ❌ | `true` = treat `filename` as a glob pattern (e.g., `*.csproj`) |

> For the full technical specification and implementation details, see `docs/specs/ast_module.md`.

---

## Customizing Framework Detection

The AST module automatically detects which frameworks your project uses (e.g., Spring, Flask, Express, Angular) by scanning decorators, class inheritance, and import paths. These detection rules are defined in **framework YAML files** that you can fully customize.

### How It Works

When Graphit Code detects frameworks, it loads framework rules from **all three levels and merges them** (unlike queries, which use precedence-based override):

1. **Runtime** (`~/.graphit/runtime/<version>/ast/frameworks/`) — Factory defaults extracted from the binary.
2. **User Global** (`~/.graphit/ast/frameworks/`) — Your personal framework rules. Apply to all projects.
3. **Project** (`.graphit/ast/frameworks/`) — Highest priority. Applies only to this project.

All levels are merged together — your project-level framework files **extend** the built-in rules rather than replacing them. This means you can add detection for your custom internal frameworks without losing detection for standard ones.

### Viewing the Defaults

After your first `graphit sync`, the default framework rules are extracted to:
```
~/.graphit/runtime/<version>/ast/frameworks/
```

Browse these files to see all built-in framework detection rules:
```bash
ls ~/.graphit/runtime/*/ast/frameworks/
# _go_lang.yaml   _java_lang.yaml   angular.yaml   django.yaml
# express.yaml    flask.yaml        nestjs.yaml    spring.yaml
# vue.yaml        ...
```

Files prefixed with `_` (e.g., `_go_lang.yaml`) contain language-level base rules including entry point scoring, while unprefixed files contain framework-specific detection rules.

### Framework YAML Schema

Each framework YAML file can contain the following sections:

```yaml
framework: my_framework        # Unique framework identifier (required)
languages: [python, javascript] # Languages this framework applies to

decorator_detection:            # Detect framework by decorator/annotation usage
  - name: MyDecorator
    category: web               # Category: web, orm, di, test, config, etc.

heritage_detection:             # Detect framework by class inheritance
  - parent: BaseController
    category: web
    framework_name: my_fw_web   # Optional: override the framework name

import_detection:               # Detect framework by import path patterns
  - pattern: "my-framework"
    match: prefix               # "prefix" (default), "exact", "contains", "suffix", or "regex"
    category: web

entry_points:                   # Entry point scoring (see next section)
  names: [...]
  decorators: [...]
  exported_bonus: 10
  max_score: 100
```

### Example: Adding Detection for a Custom Internal Framework

Suppose your team uses an internal Python RPC framework called `acme-rpc`. Create a framework file at `.graphit/ast/frameworks/acme_rpc.yaml`:

```yaml
framework: acme_rpc
languages: [python]

decorator_detection:
  - name: rpc_method
    category: rpc
  - name: rpc_service
    category: rpc

heritage_detection:
  - parent: AcmeServiceBase
    category: rpc

import_detection:
  - pattern: "acme.rpc"
    match: prefix
    category: rpc
  - pattern: "^acme\\.(rpc|grpc)\\.v[0-9]+$"
    match: regex
    category: rpc

entry_points:
  decorators:
    - name: rpc_method
      score: 70
    - name: rpc_service
      score: 50
```

After running `graphit sync`, the framework detection will identify your internal RPC framework alongside standard ones. You can then query detected frameworks via:
```cypher
MATCH (c:File {path: '__config__'}) RETURN c.lang AS frameworks
```

---

## Customizing Entry Point Scoring

The `entry_point_score` property on Function nodes indicates how likely a function is to be an application entry point (e.g., `main`, HTTP handlers, test functions). This score is computed from YAML rules defined in framework files.

### How Scoring Works

The scoring engine evaluates each function against three criteria. Scores from all matching rules are **summed together**, then capped at `max_score`:

1. **Name-based scoring** — Matches function names using glob patterns.
2. **Decorator-based scoring** — Matches specific decorators/annotations.
3. **Exported bonus** — Adds a flat bonus if the function is exported.

### Name-Based Scoring with Glob Patterns

Name patterns support four matching modes:

| Pattern | Mode | Example Match |
|---|---|---|
| `main` | Exact | `main` only |
| `Test*` | Prefix | `TestLogin`, `TestPayment` |
| `*Handler` | Suffix | `AuthHandler`, `PaymentHandler` |
| `*cmd*` | Contains | `cmdStart`, `runCmdLoop` |

> Name matching is case-insensitive. The pattern `Test*` matches both `TestFoo` and `testFoo`.

### Decorator-Based Scoring

Decorator rules match by exact name, optionally with a suffix match (e.g., a decorator `PostMapping` matches both `PostMapping` and `org.springframework.web.bind.annotation.PostMapping`).

### Configuration Fields

- **`exported_bonus`** — Flat score added to every exported/public function (default: `10`).
- **`max_score`** — Hard cap on the total score (default: `100`).

### Example: Customizing Scoring for a Project

To boost the entry point score for CLI command handlers in a Go project, create `.graphit/ast/frameworks/my_cli.yaml`:

```yaml
framework: my_project_cli
languages: [go]

entry_points:
  names:
    - pattern: "Execute*"
      score: 60
    - pattern: "Run*"
      score: 50
    - pattern: "*Cmd"
      score: 40
  decorators:
    - name: cobra.Command
      score: 70
  exported_bonus: 15
  max_score: 100
```

After `graphit sync`, you can query high-scoring entry points:
```cypher
MATCH (f:Function) WHERE f.entry_point_score > 50
RETURN f.name, f.entry_point_score, f.path
ORDER BY f.entry_point_score DESC
```

---

## Customizing Ecosystem Detection

Ecosystem detection identifies your project's build tools, package managers, and development environment by checking for the presence of well-known configuration files (e.g., `package.json`, `Cargo.toml`, `pyproject.toml`).

### What `ecosystems.yaml` Does

The `ecosystems.yaml` file contains a list of `config_files` entries. Each entry maps a config filename to a language and ecosystem identifier. When Graphit Code finds a matching file in your project root, it records the ecosystem in the AST graph's `__config__` node.

### How Resolution Works

Like frameworks, ecosystem entries **merge from all levels**:
1. **Runtime** → 2. **User Global** (`~/.graphit/ast/ecosystems.yaml`) → 3. **Project** (`.graphit/ast/ecosystems.yaml`)

All entries from every level are combined — project-level entries extend the built-in list.

### Example: Adding Custom Config File Patterns

If your organization uses a custom build system with a `build.acme` config file, create `.graphit/ast/ecosystems.yaml` in your project:

```yaml
config_files:
  - filename: build.acme
    language: go
    ecosystem: acme_build

  - filename: ".acme-ci.yaml"
    language: go
    ecosystem: acme_ci

  # Glob patterns are supported for dynamic filenames
  - filename: "*.acme"
    language: go
    ecosystem: acme_build
    glob: true
```

After `graphit sync`, the detected ecosystems will be visible in:
```cypher
MATCH (c:File {path: '__config__'}) RETURN c.source AS configs
```

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

### How to Override Per Project

To customize language configuration for a specific project, create or copy the language YAML file into `.graphit/ast/queries/`:

```bash
# Copy the default Python config as a starting point
mkdir -p .graphit/ast/queries/
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml

# Edit the export strategy, self keywords, etc.
$EDITOR .graphit/ast/queries/python.yaml
```

> Language configuration follows the same 3-level resolution chain as queries: project → user global → runtime (all YAML). The first source that provides configuration for a language wins.

---

## Adding New Language Support

Graphit Code ships with 18 built-in languages. Tree-sitter grammars (17 languages) are compiled natively into the binary via CGO, and the ANTLR PL/SQL grammar uses a native Go binary. **Adding an entirely new language grammar requires modifying the Go source code and recompiling.**

However, the YAML query files that control what gets extracted from the AST are fully customizable. You can:

- **Customize extraction queries** for any of the 18 built-in languages
- **Override export strategies, self keywords, context types**, and other language configuration
- **Add or remove entity extraction patterns** per project or globally

### What You CAN Customize Without Recompilation

All extraction behavior is driven by YAML files that follow the 3-level resolution chain:

| Priority | Path | Scope |
|----------|------|-------|
| 1 | `.graphit/ast/queries/` | Project-only |
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

### What Requires Recompilation

To add support for an entirely new language (e.g., Haskell, Elixir, Scala):

1. **Tree-sitter languages** — Add CGO bindings for the new grammar in the Go source under `internal/ast/treesitter/`
2. **ANTLR languages** — Add a native Go parser under `internal/ast/antlr/`
3. **Recompile** — Run `make install` to build the updated binary
4. **Create a YAML query file** — Define extraction patterns in a `<language>.yaml` file

The YAML query file for the new language follows the same format as existing languages and can be customized via the resolution chain after compilation.

### ANTLR Language Configuration

For languages parsed by ANTLR v4 (currently PL/SQL), the YAML configuration uses XPath expressions instead of Tree-sitter S-expressions. ANTLR grammars are compiled as native Go binaries — not loaded at runtime.

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

### Grammar Selection (`--grammar`)

By default, Tree-sitter is used when a grammar exists for the file extension; ANTLR is used as fallback. When both engines support the same extension (e.g., `.sql`), Tree-sitter is tried first and ANTLR is used only if Tree-sitter fails or extracts nothing.

To override this and select a specific grammar per extension, use the `--grammar` flag:

```bash
# Force ANTLR PL/SQL grammar for .sql files
graphit sync --grammar .sql=antlr-plsql

# Multiple overrides
graphit sync --grammar .sql=antlr-plsql,.pks=antlr-plsql,.pkb=antlr-plsql

# Force tree-sitter SQL grammar explicitly
graphit ast index --grammar .sql=tree-sitter-sql
```

The grammar name determines the backend automatically: names starting with `antlr-` use ANTLR, others use tree-sitter.

### Important Notes

- **Built-in grammars**: All 17 Tree-sitter grammars and the ANTLR PL/SQL grammar are compiled natively into the binary. Only YAML query files (extraction patterns, export strategies, language configuration) are customizable at runtime via the resolution chain.
- **Pattern validation**: Invalid Tree-sitter patterns are detected at parse time and logged as warnings, while valid patterns proceed normally. Invalid XPath expressions in ANTLR queries are similarly logged.
- **Customizing existing languages**: For the 18 languages included by default, all extraction rules, export detection, scoring, context resolution, and docstring attachment are fully YAML-driven. Changing the YAML is sufficient — no rebuild needed.
- **Parser field**: If the `parser` field is omitted from a YAML file, Tree-sitter is assumed. Existing YAML files do not need modification.

---

## Adding New Framework Support

You can add detection for any framework by creating a framework YAML file. This is useful for internal frameworks, niche libraries, or recently released tools not yet included in the defaults.

### Step-by-Step Guide

**1. Create a framework YAML file:**

For a project-level framework, create it in `.graphit/ast/frameworks/`. For a global framework (all projects), use `~/.graphit/ast/frameworks/`.

```bash
mkdir -p .graphit/ast/frameworks/
$EDITOR .graphit/ast/frameworks/fastapi.yaml
```

**2. Define detection rules:**

```yaml
framework: fastapi
languages: [python]

# Detect by decorators applied to functions/classes
decorator_detection:
  - name: app.get
    category: web
  - name: app.post
    category: web
  - name: app.put
    category: web
  - name: app.delete
    category: web
  - name: Depends
    category: di

# Detect by class inheritance
heritage_detection:
  - parent: BaseModel
    framework_name: pydantic
    category: validation

# Detect by import paths
import_detection:
  - pattern: fastapi
    match: prefix
    category: web
  - pattern: pydantic
    match: prefix
    framework_name: pydantic
    category: validation

# Score entry points specific to this framework
entry_points:
  decorators:
    - name: app.get
      score: 70
    - name: app.post
      score: 70
    - name: app.put
      score: 70
    - name: app.delete
      score: 70
    - name: Depends
      score: 30
```

**3. Run `graphit sync`:**

```bash
graphit sync
```

**4. Verify detection:**

```cypher
MATCH (c:File {path: '__config__'}) RETURN c.lang AS frameworks
```

### Tips

- **Use `framework_name`** on individual rules to attribute detection to a sub-framework (e.g., `pydantic` detected via a `fastapi` framework file).
- **Combine all three detection methods** (decorators, heritage, imports) for robust detection — different projects may use the framework in different ways.
- **Entry point scoring** in framework files is additive — if multiple framework files define scoring rules, all matching rules contribute to the final score.
- **Testing your rules**: After `graphit sync`, query for frameworks and entry points to verify that your rules produce the expected results.

---

## Customizing Module Rules and Skills

The on-demand IDE agent follows **rules** and **skills** defined per module (AST, Knowledge, Memory, Hub, and Improvements). The background Dream agent uses these artifacts as context for autonomous skill generation and knowledge mining.
Rules are the compact instructions injected into the global rules file (e.g., `AGENTS.md`). Skills are the detailed instruction files that agents read on-demand.
Graphit Code provides a **multi-layer override system** so you can customize both at different scopes.

### Override Hierarchy (highest to lowest priority)

1. **Project-Level** — `.graphit/rules/<module>.md` / `<module>_skill.md` in the project directory. Applies only to that project.
2. **Global CLI** — `~/.graphit/rules/<module>.md` / `<module>_skill.md`. Applies to all projects on your machine.
3. **Hub Main Branch** — `rules/<module>.md` / `rules/<module>_skill.md` on the `main` branch of the Hub Git repository. Applies to all team members automatically.
4. **Compiled-In Default** — Built into the Graphit Code binary.

The first source found wins. This means a project-level override always takes precedence, followed by the user's global override, then the team's Hub-distributed override, and finally the built-in default.

### Managing Rules via CLI

Every module exposes a `rule` subcommand:

```bash
# Output resolved rules (respecting the full override hierarchy)
graphit improvements rules

# Show only the compiled-in default (ignore all overrides)
graphit improvements rules --default

# Set a custom global CLI override from a file
graphit improvements rules my-rules.md

# Restore default ruleset (removes the global CLI override)
graphit improvements rules --unset
```

This works for all modules: `graphit ast rule`, `graphit knowledge rule`, `graphit memory rule`, `graphit hub rule`, `graphit improvements rules`.

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

To enforce standards across your entire team, commit rule and/or skill files to the `rules/` directory on the `main` branch of the Hub Git repository. For example:

```
hub-repo (main branch)
└── rules/
    ├── improvements.md          # team-wide improvements rule override
    ├── ast.md                   # team-wide AST rule override
    └── memory_skill.md          # team-wide memory skill override
```

Every team member will receive these overrides automatically on `graphit sync` or `graphit update` — they are distributed via git pull, without needing each developer to manually configure their machine.

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
