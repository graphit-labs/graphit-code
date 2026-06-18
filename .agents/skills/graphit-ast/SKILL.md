---
name: graphit-ast
description: AST Code Exploration and structural analysis graph queries. Use this skill for ANY code analysis: finding functions, classes, callers, imports, inheritance, unused code, and refactoring impact. This is your PRIMARY tool — use it INSTEAD OF grep_search, ripgrep, or file-by-file reading for all structural code queries. Also use when spawning subagents that need to explore code.
---

# Code Exploration via AST Rule

## 🔒 MANDATORY: AST Graph Is Your PRIMARY Code Analysis Tool

**The AST graph database is your DEFAULT and OBLIGATORY mechanism for any
code analysis, exploration, or understanding task.** It must be consulted
BEFORE you use any textual search tools (ripgrep, grep, semantic search,
code symbols, file-by-file reading, or any other text-based mechanism).

### Why AST-first?

| Property | AST Graph | Textual Search |
|---|---|---|
| **Speed** | ⚡ Sub-millisecond — indexed graph traversal | 🐢 Scans files sequentially |
| **Determinism** | ✅ Exact, reproducible, structural matches | ❌ Heuristic, regex-dependent |
| **Scope** | 🌐 Holistic system understanding (calls, imports, inheritance, containment) | 📄 Per-file, no cross-file relationships |
| **Accuracy** | 🎯 Semantically precise (functions, classes, methods, parameters) | ⚠️ Prone to false positives (comments, strings, variable names) |

The AST graph holds the **complete structural model of the entire codebase**:
every function, class, method, variable, import, call relationship, and
containment hierarchy — pre-indexed and instantly queryable.

### Why this replaces your tools

| Your tool | AST equivalent | Why AST wins |
|---|---|---|
| `grep -r "functionName" src/` | Call `graphit_ast_query` with `query: "MATCH (f:Function {name: 'functionName'}) RETURN f.path, f.line_number"` | AST: O(1) indexed lookup. Grep: O(n) scan, false positives in comments/strings |
| Semantic search for "who calls X" | Call `graphit_ast_query` with `query: "MATCH (a)-[:CALLS]->(b {name: 'X'}) RETURN a.name, a.path"` | AST: exact CALLS edges. Semantic: guesses from text proximity |
| IDE code symbols / go-to-definition | Call `graphit_ast_query` with `query: "MATCH (n:Class {name: 'X'}) RETURN n.path, n.line_number"` | AST: works across files, languages, and imported contexts |
| Reading files to understand structure | Call `graphit_ast_query` with `query: "MATCH (f:File {path: 'X'})-[:CONTAINS]->(e) RETURN label(e), e.name"` | AST: instant file skeleton. File reading: manual, token-heavy |
| `grep` for import usage | Call `graphit_ast_query` with `query: "MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'X'}) RETURN f.path"` | AST: pre-resolved import graph. Grep: regex on `import` statements |
| Searching for class hierarchy | Call `graphit_ast_query` with `query: "MATCH (a:Class)-[:INHERITS*]->(b:Class {name: 'X'}) RETURN a.name"` | AST: transitive closure in one query. Manual: file-by-file tracing |

### 🔒 When you MUST use the AST graph (MANDATORY — no exceptions)

To execute any Cypher queries below, call the `graphit_ast_query` tool (passing absolute `project_dir`):

| Scenario | What to do (Cypher query) | What NOT to do |
|---|---|---|
| **Finding where a function is defined** | `MATCH (f:Function {name: 'X'}) RETURN f.path, f.line_number` | ❌ Don't grep for `function X` or `def X` |
| **Finding who calls a function** | `MATCH (a)-[:CALLS]->(b:Function {name: 'X'}) RETURN a.name, a.path` | ❌ Don't grep for the function name across all files |
| **Understanding what a function calls** | `MATCH (a:Function {name: 'X'})-[:CALLS]->(b) RETURN b.name, label(b)` | ❌ Don't read the function source and manually trace calls |
| **Finding all classes in a module** | `MATCH (f:File)-[:CONTAINS]->(c:Class) WHERE f.path STARTS WITH 'src/services/' RETURN c.name` | ❌ Don't list directory and read each file |
| **Tracing class inheritance** | `MATCH (a:Class)-[:INHERITS*]->(b:Class {name: 'Base'}) RETURN a.name, a.path` | ❌ Don't grep for `extends Base` across files |
| **Finding imports of a module** | `MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'react'}) RETURN f.path` | ❌ Don't grep for `import.*react` |
| **Assessing impact of a change** | Query all CALLS/IMPORTS/INHERITS edges pointing to the changed entity | ❌ Don't manually search for usages file by file |
| **Understanding file structure** | `MATCH (f:File {path: 'X'})-[:CONTAINS]->(e) RETURN label(e), e.name, e.line_number` | ❌ Don't read the entire file to understand its structure |
| **Finding DML dependencies** | `MATCH (p:Procedure)-[:SELECTS]->(t:Table) RETURN p.name, t.name` | ❌ Don't grep for `SELECT.*FROM` in SQL files |
| **Checking complexity** | `MATCH (f:Function) WHERE f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity` | ❌ Don't manually count branches in source code |
| **Finding unused code** | `MATCH (f:Function) WHERE NOT ()-[:CALLS]->(f) RETURN f.name, f.path` | ❌ Don't grep for function names across the entire project |
| **Refactoring impact analysis** | Query all inbound edges (CALLS, INHERITS, IMPORTS) to the target entity | ❌ Don't assume you know all usages from reading a few files |
| **Finding entry points** | `MATCH (f:Function) WHERE f.entry_point_score > 50 RETURN f.name, f.path` | ❌ Don't guess from naming conventions — graph has pre-computed scores |
| **Identifying project frameworks** | `MATCH (c:File {path: '__config__'}) RETURN c.lang` | ❌ Don't scan files manually — graph has auto-detected frameworks |
| **Tracing self/this calls** | `MATCH (a)-[r:CALLS]->(b) WHERE r.receiver_type <> '' RETURN a.name, b.name, r.receiver_type` | ❌ Don't read source to trace method dispatch — graph has receiver_type on CALLS edges |
| **Finding interface implementors** | `MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface {name: 'X'}) RETURN c.name` | ❌ Don't grep for `implements X` — graph has IMPLEMENTS edges |

### When you should NOT use the AST graph

| Scenario | Use instead |
|---|---|
| Reading a file whose path you already know | Your native IDE file-reading tools (view_file, etc.) — faster and simpler |
| Searching inside string literals or comments | grep/ripgrep on source files |
| Editing source code | File edit tools |
| Running tests or builds | Terminal commands |
| Checking runtime behavior or logs | Terminal/browser tools |
| Understanding project documentation | Knowledge wiki (not AST) |

## Phased Graph Exploration

You have access to a LadybugDB graph database with the entire project's AST.
Use the following multi-phase workflow to explore it:


### Phase 1: Know the schema

Node labels are dynamic (projects may have different entity types). To discover
which labels and relationships exist in the current graph, call the `graphit_ast_schema` tool (passing `project_dir`).

However, **property names are fixed and universal.** You MUST use the exact
property names listed below — inventing names will crash queries.

#### Property Reference (MEMORIZE — never guess property names)

| Node Type | Properties |
|---|---|
| **File** | `path`, `name`, `relative_path`, `is_dependency`, `lang`, `cluster`, `source` |
| **Directory** | `path`, `name`, `cluster` |
| **Function, Method, Class, Struct, Interface, Type, Variable, Constant, Field, Parameter** | `uid`, `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`, `cyclomatic_complexity`, `context`, `context_type`, `class_context`, `is_dependency`, `is_exported`, `value`, `is_stub`, `entry_point_score`, `cluster` |
| **Module** | `uid`, `name`, `lang`, `full_import_name`, `path`, `line_number`, `end_line`, `docstring`, `cyclomatic_complexity`, `context`, `context_type`, `is_dependency`, `is_exported`, `is_stub`, `cluster` |

| Relationship | Properties |
|---|---|
| **CALLS** | `source_file`, `line_number`, `full_call_name`, `receiver_type` |
| **CONTAINS** | *(none)* |
| **IMPORTS** | `alias`, `full_import_name`, `imported_name`, `line_number`, `source_file` |
| **HAS_FIELD, HAS_PARAMETER** | `source_file`, `line_number` |
| **READS_FIELD, WRITES_FIELD** | `source_file`, `line_number` |

### Phase 2: Pre-search (Grounding)

**Never guess entity names.** Use a loose text search first by calling `graphit_ast_query`:
```
query: "MATCH (n) WHERE toLower(n.name) CONTAINS toLower('keyword') RETURN DISTINCT n.name as name, label(n) as label"
```
This prevents wasted queries on misspelled or assumed names.

> ⚠️ **CRITICAL: The node type is its **graph label** (e.g., `Function`, `Class`, `Method`).
> To get the node type, use the built-in **`label(n)`** function, NOT a property access.
> Writing `RETURN n.kind` or `RETURN n.type` will crash with `Cannot find property`.
> ✅ Correct: `RETURN label(n) AS type`
> ❌ Wrong: `RETURN n.kind` / `RETURN n.type` / `RETURN n.label`

### Phase 2.3: Hybrid Search (RECOMMENDED — Best Results)

**This is the RECOMMENDED default for text-based discovery.** It combines BM25 full-text
search with semantic vector search using Reciprocal Rank Fusion (RRF, k=60) to produce
a unified ranking — call the `graphit_ast_search` tool (passing absolute `project_dir` and `query`):
```
graphit_ast_search(project_dir: "/path/to/project", query: "authentication and session management")
```

Hybrid search automatically:
- Splits code identifiers (e.g., `handleHTTPRequest` → `handle HTTP Request`) for better FTS matching
- Runs multi-pass BM25 (phrase, AND, OR, prefix, trigram) + semantic vector similarity
- Fuses results via RRF where exact matches rank higher than semantic, but semantic boosts partial matches
- Falls back to FTS-only when embeddings are unavailable

Use the optional `mode` parameter to restrict to a single search type:
- `mode: "hybrid"` (default) — combined FTS + semantic via RRF
- `mode: "fts"` — BM25 keyword search only
- `mode: "semantic"` — vector similarity only

> ⚠️ **CRITICAL: `graphit_ast_search` accepts PLAIN TEXT only — NEVER Cypher.**
> The query string is used as keywords or natural language to find similar code.
> Passing a Cypher `MATCH` statement will return garbage.
> ✅ Correct: Call `graphit_ast_search` with `query: "payment processing"`
> ❌ Wrong: Call `graphit_ast_search` with `query: "MATCH (f:Function) WHERE ..."`

**Important:** Semantic mode requires embeddings to have been computed.
If semantic results are empty, call the `graphit_sync` tool (passing `project_dir`) to generate embeddings — fire-and-forget, do not wait for it to finish.
In hybrid mode, it gracefully falls back to FTS-only when embeddings are unavailable.

### Phase 3: Precise Graph Query

Once you know the exact names and labels from Phase 2, construct the final query. Call the `graphit_ast_query` tool (passing `project_dir` and `query`):

> ⚠️ **Multi-label search — DEFAULT BEHAVIOR when searching by name:**
> Many entities share the same name but differ only in label. Languages have subtle
> distinctions that you CANNOT reliably predict from the name alone:
> - **Function vs Method**: In Go, `func Search(...)` is a Function, but `func (idx *BM25Index) Search(...)` is a Method. In Python/Java, class methods are Methods, standalone ones are Functions.
> - **Class vs Struct**: In Go/Rust, `type X struct` is a Struct, not a Class. In Python/Java, it's a Class.
> - **Interface vs Trait**: In Go, `type X interface` is an Interface. In Rust, `trait X` is a Trait.
> - **Variable vs Constant**: `var x` vs `const x` — same name, different labels.
>
> **RULE: When searching for an entity by name and you are NOT 100% certain of its exact label,
> ALWAYS use a multi-label query.** This is the SAFE DEFAULT — a single-label query risks missing results silently.
> ```
> # Instead of: MATCH (f:Function {name: 'Search'}) ...  (MISSES methods!)
> # Use:
> Call graphit_ast_query with query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND f.name = 'Search' RETURN f.name, f.path, f.line_number, label(f) AS type"
> ```
>
> **Common multi-label pairs to always consider:**
> | Looking for | Use these labels |
> |---|---|
> | A callable (function/method) | `label(f) = 'Function' OR label(f) = 'Method'` |
> | A type definition (class/struct) | `label(f) = 'Class' OR label(f) = 'Struct'` |
> | An abstraction (interface/trait) | `label(f) = 'Interface' OR label(f) = 'Trait'` |
> | A named value (variable/constant) | `label(f) = 'Variable' OR label(f) = 'Constant'` |
> | Anything — full discovery | `MATCH (n) WHERE n.name = 'X' RETURN n.name, label(n) AS type, n.path` |

Common queries to pass in `query` parameter:
```bash
# Who calls this function?
MATCH (a:Function)-[:CALLS]->(b:Function {name: 'ExactName'}) RETURN a.name, a.path

# What does this function call?
MATCH (a:Function {name: 'ExactName'})-[:CALLS]->(b) RETURN b.name, label(b)

# Class inheritance chain
MATCH (a:Class)-[:INHERITS*]->(b:Class {name: 'BaseClass'}) RETURN a.name, a.path

# All entities in a file
MATCH (f:File {path: 'src/main.go'})-[:CONTAINS]->(e) RETURN label(e) as type, e.name, e.line_number ORDER BY e.line_number

# Import graph — who imports this module?
MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'react'}) RETURN f.path

# DML dependencies — what tables does this procedure touch?
MATCH (p:Procedure {name: 'processOrder'})-[r]->(t:Table) RETURN type(r), t.name

# Find unused functions (no callers)
MATCH (f:Function) WHERE NOT ()-[:CALLS]->(f) RETURN f.name, f.path

# High-complexity functions
MATCH (f:Function) WHERE f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# Entry points — functions scored as application entry points (main, handlers, tests)
MATCH (f:Function) WHERE f.entry_point_score > 50 RETURN f.name, f.entry_point_score, f.path ORDER BY f.entry_point_score DESC

# Receiver type — trace self/this method calls to their owning class
MATCH (a:Function)-[r:CALLS]->(b:Function) WHERE r.receiver_type IS NOT NULL AND r.receiver_type <> '' RETURN a.name, b.name, r.receiver_type

# Interface implementations — who implements interface X?
MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface {name: 'Handler'}) RETURN c.name, c.path

# Project config & detected frameworks
MATCH (c:File {path: '__config__'}) RETURN c.source AS configs, c.lang AS frameworks
```

### 📖 Graph Exploration Cookbook — IDE-like Operations

These examples cover every common code exploration pattern that developers
use in IDEs. **Always prefer these graph queries over text-based tools.**

#### 1. Find Usages — Who uses this method/function?

Query templates to run with `graphit_ast_query`:
```bash
# Direct callers of a function/method
MATCH (caller)-[:CALLS]->(target:Function {name: 'ProcessPayment'}) RETURN caller.name, label(caller) AS type, caller.path

# All callers, including indirect (transitive call chain up to 3 levels)
MATCH (caller)-[:CALLS*1..3]->(target:Function {name: 'Validate'}) RETURN DISTINCT caller.name, caller.path

# Who uses a class? (instantiation, inheritance, field type references)
MATCH (user)-[r]->(c:Class {name: 'UserService'}) RETURN user.name, label(user) AS user_type, type(r) AS relationship

# Who uses a module/package? (import tracking)
MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'express'}) RETURN f.path, f.name
```

#### 2. Find Implementors — Who implements an interface/trait?

Query templates to run with `graphit_ast_query`:
```bash
# Direct implementors of an interface
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Repository'}) RETURN impl.name, label(impl) AS type, impl.path

# All implementors of a trait (works for Go interfaces, Java interfaces, Dart abstract classes)
MATCH (impl)-[:IMPLEMENTS]->(t:Trait {name: 'Serializable'}) RETURN impl.name, impl.path

# Find parent interface usages — who calls methods of ANY implementor of interface X?
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Handler'}) MATCH (caller)-[:CALLS]->(m:Function) WHERE m.path = impl.path RETURN caller.name, caller.path, impl.name AS implementor

# Interface + all concrete method dispatch (receiver_type tracking)
MATCH (caller)-[r:CALLS]->(method:Function) WHERE r.receiver_type CONTAINS 'Service' RETURN caller.name, method.name, r.receiver_type, caller.path
```

#### 3. Call Hierarchy — Upstream and Downstream

Query templates to run with `graphit_ast_query`:
```bash
# Outgoing calls — what does this function call? (call tree downward)
MATCH (f:Function {name: 'HandleRequest'})-[:CALLS]->(callee) RETURN callee.name, label(callee) AS type

# Incoming calls — who calls this function? (call tree upward)
MATCH (caller)-[:CALLS]->(f:Function {name: 'SaveOrder'}) RETURN caller.name, label(caller) AS type, caller.path

# Full bidirectional call context (called by + calls)
MATCH (caller)-[:CALLS]->(f:Function {name: 'ProcessItem'}) RETURN 'called_by' AS direction, caller.name, caller.path UNION ALL MATCH (f:Function {name: 'ProcessItem'})-[:CALLS]->(callee) RETURN 'calls' AS direction, callee.name, callee.path

# Method call chain — trace self/this method calls through receiver types
MATCH (a:Function)-[r:CALLS]->(b:Function) WHERE r.receiver_type = 'OrderService' RETURN a.name AS caller, b.name AS method, a.path
```

#### 4. Inheritance & Type Hierarchy

Query templates to run with `graphit_ast_query`:
```bash
# Direct subclasses of a class
MATCH (child:Class)-[:INHERITS]->(parent:Class {name: 'BaseController'}) RETURN child.name, child.path

# Full inheritance chain (transitive — all ancestors)
MATCH (c:Class {name: 'AdminController'})-[:INHERITS*]->(ancestor:Class) RETURN ancestor.name, ancestor.path

# Full inheritance tree (transitive — all descendants of a base class)
MATCH (descendant:Class)-[:INHERITS*]->(base:Class {name: 'AbstractEntity'}) RETURN descendant.name, descendant.path

# Combined: class hierarchy + interface implementations
MATCH (c:Class {name: 'UserRepository'})-[r]->(target) WHERE type(r) IN ['INHERITS', 'IMPLEMENTS'] RETURN type(r) AS relation, target.name, label(target) AS target_type
```

#### 5. Containment — File/Class/Package Structure

Query templates to run with `graphit_ast_query`:
```bash
# All entities defined in a file (file skeleton)
MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path ENDS WITH 'service.go' RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number

# All methods of a class
MATCH (c:Class {name: 'OrderService'})-[:CONTAINS]->(m:Function) RETURN m.name, m.line_number, m.is_exported

# All fields/properties of a class
MATCH (c:Class {name: 'User'})-[:HAS_FIELD]->(f:Field) RETURN f.name, f.value

# All classes in a directory/module
MATCH (f:File)-[:CONTAINS]->(c:Class) WHERE f.path STARTS WITH 'src/services/' RETURN c.name, f.path

# Package/namespace structure
MATCH (p:Package)-[:CONTAINS]->(e) RETURN p.name AS package, label(e) AS type, e.name
```

#### 6. Field & Property Access Tracking

Query templates to run with `graphit_ast_query`:
```bash
# Who reads a specific field?
MATCH (reader)-[:READS_FIELD]->(f:Field {name: 'balance'}) RETURN reader.name, label(reader) AS type, reader.path

# Who writes/modifies a specific field?
MATCH (writer)-[:WRITES_FIELD]->(f:Field {name: 'status'}) RETURN writer.name, label(writer) AS type, writer.path

# All field access (read + write) for a given entity
MATCH (accessor)-[r]->(f:Field {name: 'email'}) WHERE type(r) IN ['READS_FIELD', 'WRITES_FIELD'] RETURN accessor.name, type(r) AS access_type, accessor.path
```

#### 7. DML & Database Dependency Tracking

Query templates to run with `graphit_ast_query`:
```bash
# What tables does a procedure/function read from?
MATCH (p:Procedure {name: 'getCustomerOrders'})-[:SELECTS]->(t:Table) RETURN t.name

# What procedures INSERT into a specific table? (write impact)
MATCH (writer)-[:INSERTS]->(t:Table {name: 'audit_log'}) RETURN writer.name, label(writer) AS type, writer.path

# Full DML dependency map for a table (who reads, writes, updates, deletes?)
MATCH (entity)-[r]->(t:Table {name: 'orders'}) WHERE type(r) IN ['SELECTS', 'INSERTS', 'UPDATES', 'DELETES'] RETURN entity.name, type(r) AS operation, label(entity) AS entity_type

# DDL impact — who creates/alters/drops this table?
MATCH (entity)-[r]->(t:Table {name: 'users'}) WHERE type(r) IN ['CREATES', 'ALTERS', 'DROPS'] RETURN entity.name, type(r) AS ddl_op, entity.path
```

#### 8. Refactoring Impact Analysis

Query templates to run with `graphit_ast_query`:
```bash
# COMPLETE impact of renaming/changing a function — all inbound edges
MATCH (dependent)-[r]->(target:Function {name: 'calculateTotal'}) RETURN dependent.name, label(dependent) AS dep_type, type(r) AS relation, dependent.path

# COMPLETE impact of changing an interface — implementors + callers of implementors
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) RETURN impl.name, impl.path UNION ALL MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) MATCH (caller)-[:CALLS]->(m:Function) WHERE m.path = impl.path RETURN caller.name AS name, caller.path AS path

# Safe-to-delete check — is this function called anywhere?
MATCH (f:Function {name: 'legacyHelper'}) OPTIONAL MATCH (caller)-[:CALLS]->(f) RETURN f.name, f.path, count(caller) AS caller_count

# Move-file impact — all entities in a file and their external dependents
MATCH (f:File {path: 'src/utils/helpers.go'})-[:CONTAINS]->(entity) OPTIONAL MATCH (external)-[r]->(entity) WHERE external.path <> 'src/utils/helpers.go' RETURN entity.name, label(entity) AS type, count(external) AS external_deps
```

#### 9. Cross-Cutting Queries

Query templates to run with `graphit_ast_query`:
```bash
# Find all entry points (handlers, main functions, test functions)
MATCH (f:Function) WHERE f.entry_point_score > 50 RETURN f.name, f.entry_point_score, f.path ORDER BY f.entry_point_score DESC

# Find all functions with high complexity (candidates for refactoring)
MATCH (f:Function) WHERE f.cyclomatic_complexity > 15 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# Find orphan functions (never called — dead code candidates)
MATCH (f:Function) WHERE NOT ()-[:CALLS]->(f) AND f.entry_point_score < 10 RETURN f.name, f.path

# Cross-language dependencies (e.g., Go calling a function defined in SQL)
MATCH (caller)-[:CALLS]->(callee) WHERE caller.lang <> callee.lang RETURN caller.name, caller.lang, callee.name, callee.lang, caller.path

# Find circular dependencies between files
MATCH (a:File)-[:IMPORTS]->(m1:Module)<-[:CONTAINS]-(b:File)-[:IMPORTS]->(m2:Module)<-[:CONTAINS]-(a) WHERE a.path < b.path RETURN a.path, b.path

# Annotation/decorator usage — find all entities with a specific annotation
MATCH (a:Annotation {name: 'Deprecated'})<-[:CONTAINS]-(owner) RETURN label(owner) AS type, owner.name, owner.path

# Parameter analysis — what parameters does a function expect?
MATCH (f:Function {name: 'createUser'})-[:HAS_PARAMETER]->(p:Parameter) RETURN p.name, p.value, p.line_number
```

### Phase 4: Source Code Extraction

The AST graph stores the **complete source code** of every indexed file. The `graphit_ast_source` tool
provides IDE-like capabilities to navigate source code efficiently — equivalent to `grep`, `head`, `tail`, and more.

> **⚠️ IMPORTANT: If you already know the file path** and just need to read its content,
> **use your native IDE file-reading tools** (e.g., `view_file`). They are faster and simpler.
> Use `graphit_ast_source` when you **discovered** a file through an AST query and want its content in the same round-trip,
> or when you need **advanced slicing** (entity extraction, pattern search with context).

**Imported ast contexts** also may contain **source code** included in their imported graph,
try querying for it to understand the imported code and the overall behavior of the external contexts.

#### 4a. Get the entire source of a file

Call `graphit_ast_source` (passing absolute `project_dir` and `path`):
```
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go")
```

#### 4b. Get source with line numbers

```
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", line_numbers: true)
```

#### 4c. Head / Tail — view first or last N lines

```bash
# First 20 lines of a file
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", head: 20)

# Last 30 lines of a file
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", tail: 30)
```

#### 4d. Line range — extract specific lines

```
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", start_line: 50, end_line: 80)
```

#### 4e. Entity extraction — get source of a function/class/method by name

Extracts the source using the entity's `line_number` and `end_line` from the graph — **no need for a two-step query**:
```bash
# Extract a function by name
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", entity: "ValidateToken")

# Disambiguate when multiple entities share the same name
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", entity: "ValidateToken", entity_type: "Function")
```

#### 4f. Pattern search — grep-like search within a file

```bash
# Search for a literal text pattern
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", pattern: "error")

# Regex pattern with context lines (before/after)
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", pattern: "func.*Validate", regex: true, before: 2, after: 5)
```

#### 4g. Combined — entity + pattern (search within entity source)

```
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go", entity: "ValidateToken", pattern: "error", before: 1, after: 1)
```

#### 4h. Quick source peek — one-shot via `graphit_ast_query`

When you want metadata + source in a single call:
```
MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN fn.name, fn.line_number, fn.end_line, file.path, file.source
```

## Cypher Guidelines

- **IMPORTANT**: The `path` property is ALWAYS a relative path from the project root (e.g., `src/main.go`). Never use absolute paths when filtering `n.path`.
- **Node type is a LABEL, not a property.** Use `label(n)` — see Phase 1 for details.
- **Property names are exact.** Refer to the Property Reference table in Phase 1. NEVER guess property names — if it's not in the table, it doesn't exist.
- **Shared properties only.** When matching unlabeled nodes (e.g., `MATCH (n) WHERE ...`), you may ONLY access properties shared by ALL labels: `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`. For label-specific properties (e.g., `cyclomatic_complexity`, `is_exported`, `source`), you MUST specify the label in the MATCH (e.g., `(n:Function)`, `(f:File)`).
- LadybugDB strict typing: DO NOT access properties unless you explicitly MATCH the label that contains them. If a property is not shared by ALL possible labels in a pattern, LadybugDB will crash!
- **Multi-label by default.** When searching for an entity by name, NEVER hardcode a single label unless you have already confirmed the exact label from a prior query. Languages have subtle distinctions — Go uses Function vs Method, Class vs Struct; Rust uses Trait vs Interface. Use multi-label patterns: `MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND f.name = 'X'`. A query that returns extra rows is far better than one that silently misses results.
- Return only what you need. Avoid returning entire node objects (`RETURN n`); instead, return specific properties (`RETURN n.name, n.path`) to keep output concise.
- Use `LIMIT` only when you want to cap results — don't add it by default.
- Do NOT use the `context` parameter on tools when working with the current project. Only specify `context` when querying an imported third-party AST context.

## Cluster Filtering

When indexing, all nodes are tagged with a `cluster` property.

### Querying by cluster:
```
Call graphit_ast_query with query: "MATCH (n:Function {cluster: 'backend'}) RETURN n.name, n.path"
```

## 🔄 Fallback to Built-In Tools — ONLY for What the Graph Does Not Contain

**Your built-in tools (grep, ripgrep, semantic search, code symbols) are permitted
ONLY for information that the AST graph structurally cannot contain.**
The graph is ALWAYS your primary source. It is NOT a "first attempt" — it is
the definitive structural model. Your tools exist only for non-structural queries.

Your tools are allowed ONLY when ALL of these conditions are true:

1. You **already queried the AST graph** for the information using the correct tools
2. The graph **genuinely cannot answer** (e.g., string literal content, comment text, runtime values)
3. You **state explicitly** to the user: "The AST graph cannot answer X, falling back to text search"

**If even ONE of these conditions is not met, you MUST NOT use your tools.**

## ⚡ MANDATORY: Sync After Every File Modification

**After ANY modification to ANY source code file (edit, create, rename, or delete),
you MUST trigger a project sync by calling the `graphit_sync` tool (passing absolute `project_dir`):**
```
graphit_sync(project_dir: "/path/to/project")
```

**This is NON-NEGOTIABLE.** The framework depends on an up-to-date index to function.
Without syncing, the AST graph becomes stale and subsequent queries return
outdated or incomplete results — breaking the analysis pipeline.

**Rules:**
- Call `graphit_sync` immediately after any source code file modifications — **fire-and-forget: do NOT wait for sync to complete, continue working immediately.**
- **Forgetting to call sync is a framework integrity violation.**

