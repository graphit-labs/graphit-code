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
| `grep -r "functionName" src/` | `MATCH (f:Function {name: 'functionName'}) RETURN f.path` | AST: O(1) indexed lookup. Grep: O(n) scan, false positives in comments/strings |
| Semantic search for "who calls X" | `MATCH (a)-[:CALLS]->(b {name: 'X'}) RETURN a.name, a.path` | AST: exact CALLS edges. Semantic: guesses from text proximity |
| IDE code symbols / go-to-definition | `MATCH (n:Class {name: 'X'}) RETURN n.path, n.line_number` | AST: works across files, languages, and imported contexts |
| Reading files to understand structure | `MATCH (f:File {path: 'X'})-[:CONTAINS]->(e) RETURN label(e), e.name` | AST: instant file skeleton. File reading: manual, token-heavy |
| `grep` for import usage | `MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'X'}) RETURN f.path` | AST: pre-resolved import graph. Grep: regex on `import` statements |
| Searching for class hierarchy | `MATCH (a:Class)-[:INHERITS*]->(b:Class {name: 'X'}) RETURN a.name` | AST: transitive closure in one query. Manual: file-by-file tracing |

### 🔒 When you MUST use the AST graph (MANDATORY — no exceptions)

| Scenario | What to do | What NOT to do |
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
| Reading exact source code of a specific function | File-reading tools (after getting path+line from AST) |
| Searching inside string literals or comments | grep/ripgrep on source files |
| Editing source code | File edit tools |
| Running tests or builds | Terminal commands |
| Checking runtime behavior or logs | Terminal/browser tools |
| Understanding project documentation | Knowledge wiki (not AST) |

## Phased Graph Exploration

You have access to a LadybugDB graph database with the entire project's AST.
Use the following multi-phase workflow to explore it:

### 🔒 MANDATORY: Always use `--ai-optimized` output

**You MUST append `--ai-optimized` to EVERY `graphit ast query` command.**
This flag outputs results in a compact, token-efficient tabular format (TOON)
instead of verbose JSON. It reduces token consumption by 30-60%.

**TOON output format:**
```
results[<count>]{<col1>|<col2>|<col3>}:
  <val1>|<val2>|<val3>
  <val1>|<val2>|<val3>
```
Headers are declared once in the header line, then each row is pipe-separated.
Empty values are represented as empty strings between pipes.
Nested arrays use `[a,b,c]` syntax, nested maps use `{k:v,k:v}` syntax.

**Example:** `graphit ast query "MATCH (f:Function) RETURN f.name, f.path" --ai-optimized` produces:
```
results[3]{f.name|f.path}:
  main|src/main.go
  handleAuth|src/auth.go
  validate|src/validate.go
```
instead of ~30 lines of JSON with repeated keys, braces, and quotes.

### Phase 1: Know the schema

Node labels are dynamic (projects may have different entity types). To discover
which labels and relationships exist in the current graph, run:
```bash
graphit ast schema
```

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

**Never guess entity names.** Use a loose text search first to find exact names:
```bash
graphit ast query "MATCH (n) WHERE toLower(n.name) CONTAINS toLower('keyword') RETURN DISTINCT n.name as name, label(n) as label" --ai-optimized
```
This prevents wasted queries on misspelled or assumed names.

> ⚠️ **CRITICAL: The node type is its **graph label** (e.g., `Function`, `Class`, `Method`).
> To get the node type, use the built-in **`label(n)`** function, NOT a property access.
> Writing `RETURN n.kind` or `RETURN n.type` will crash with `Cannot find property`.
> ✅ Correct: `RETURN label(n) AS type`
> ❌ Wrong: `RETURN n.kind` / `RETURN n.type` / `RETURN n.label`

### Phase 2.5: Semantic Search (Intent-Based Discovery)

When grounding (Phase 2) returns no results — or when your search is
**conceptual/intent-based** rather than name-based — use semantic search:
```bash
graphit ast query "authentication and session management" --semantic --ai-optimized
graphit ast query "error handling and retry logic" --semantic --top 20 --ai-optimized
```

> ⚠️ **CRITICAL: `--semantic` accepts PLAIN TEXT only — NEVER Cypher.**
> The query string is used as a natural-language description to find similar code
> via vector embeddings. Passing a Cypher `MATCH` statement will return garbage.
> ✅ Correct: `graphit ast query "payment processing" --semantic --ai-optimized`
> ❌ Wrong: `graphit ast query "MATCH (f:Function) WHERE ..." --semantic --ai-optimized`

Semantic search uses **vector embeddings** (via sqlite-vec cosine similarity) to find
entities by meaning, not just name. Use it when:
- You don't know the exact function/class name
- You're exploring a concept across the codebase (e.g., "payment processing")
- Structural queries return too few results
- The user describes intent rather than specific code entities

**Important:** Semantic search requires embeddings to have been computed.
If results are empty, run `graphit sync &` to start background sync and generate embeddings for future use.

### Phase 2.7: Full-Text Search (Keyword-Based Discovery)

When you need to find code by **exact keywords, function names, SQL fragments,
or source code patterns** — use full-text search:
```bash
graphit ast query "processOrder" --fts --ai-optimized
graphit ast query "SELECT FROM users" --fts --top 15 --ai-optimized
```

> ⚠️ **CRITICAL: `--fts` accepts PLAIN TEXT only — NEVER Cypher.**
> The query string is used as keywords for BM25 full-text ranking.
> Passing a Cypher `MATCH` statement will NOT search source code — it will
> try to match the literal Cypher text and return nothing.
> ✅ Correct: `graphit ast query "payment" --fts --ai-optimized`
> ❌ Wrong: `graphit ast query "MATCH (f:File) WHERE toLower(f.source) CONTAINS 'payment'" --fts --ai-optimized`

FTS uses **BM25 scoring** to rank results by keyword relevance. Use it when:
- You know the exact keyword or identifier name
- You're searching for SQL fragments, error strings, or code patterns
- Semantic search is not available (embeddings not computed)
- You want faster results than semantic search (no model inference required)

**FTS also indexes `:File` source content.** This means you can search for any
text pattern that appears in source files — error messages, string literals,
comments, or code fragments — without using grep:
```bash
# Search for a string literal or error message across all source files
graphit ast query "connection refused" --fts --ai-optimized

# Find files containing a specific code pattern
graphit ast query "TODO refactor" --fts --top 20 --ai-optimized
```

**FTS vs Semantic:** Use `--fts` for exact keyword matching, `--semantic` for
meaning-based discovery. They are complementary — try both when uncertain.

### Phase 3: Precise Graph Query

Once you know the exact names and labels from Phase 2, construct the final query.
Common patterns:

```bash
# Who calls this function?
graphit ast query "MATCH (a:Function)-[:CALLS]->(b:Function {name: 'ExactName'}) RETURN a.name, a.path" --ai-optimized

# What does this function call?
graphit ast query "MATCH (a:Function {name: 'ExactName'})-[:CALLS]->(b) RETURN b.name, label(b)" --ai-optimized

# Class inheritance chain
graphit ast query "MATCH (a:Class)-[:INHERITS*]->(b:Class {name: 'BaseClass'}) RETURN a.name, a.path" --ai-optimized

# All entities in a file
graphit ast query "MATCH (f:File {path: 'src/main.go'})-[:CONTAINS]->(e) RETURN label(e) as type, e.name, e.line_number ORDER BY e.line_number" --ai-optimized

# Import graph — who imports this module?
graphit ast query "MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'react'}) RETURN f.path" --ai-optimized

# DML dependencies — what tables does this procedure touch?
graphit ast query "MATCH (p:Procedure {name: 'processOrder'})-[r]->(t:Table) RETURN type(r), t.name" --ai-optimized

# Find unused functions (no callers)
graphit ast query "MATCH (f:Function) WHERE NOT ()-[:CALLS]->(f) RETURN f.name, f.path" --ai-optimized

# High-complexity functions
graphit ast query "MATCH (f:Function) WHERE f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC" --ai-optimized

# Entry points — functions scored as application entry points (main, handlers, tests)
graphit ast query "MATCH (f:Function) WHERE f.entry_point_score > 50 RETURN f.name, f.entry_point_score, f.path ORDER BY f.entry_point_score DESC" --ai-optimized

# Receiver type — trace self/this method calls to their owning class
graphit ast query "MATCH (a:Function)-[r:CALLS]->(b:Function) WHERE r.receiver_type IS NOT NULL AND r.receiver_type <> '' RETURN a.name, b.name, r.receiver_type" --ai-optimized

# Interface implementations — who implements interface X?
graphit ast query "MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface {name: 'Handler'}) RETURN c.name, c.path" --ai-optimized

# Project config & detected frameworks
graphit ast query "MATCH (c:File {path: '__config__'}) RETURN c.source AS configs, c.lang AS frameworks" --ai-optimized
```

### 📖 Graph Exploration Cookbook — IDE-like Operations

These examples cover every common code exploration pattern that developers
use in IDEs. **Always prefer these graph queries over text-based tools.**

#### 1. Find Usages — Who uses this method/function?

```bash
# Direct callers of a function/method
graphit ast query "MATCH (caller)-[:CALLS]->(target:Function {name: 'ProcessPayment'}) RETURN caller.name, label(caller) AS type, caller.path" --ai-optimized

# All callers, including indirect (transitive call chain up to 3 levels)
graphit ast query "MATCH (caller)-[:CALLS*1..3]->(target:Function {name: 'Validate'}) RETURN DISTINCT caller.name, caller.path" --ai-optimized

# Who uses a class? (instantiation, inheritance, field type references)
graphit ast query "MATCH (user)-[r]->(c:Class {name: 'UserService'}) RETURN user.name, label(user) AS user_type, type(r) AS relationship" --ai-optimized

# Who uses a module/package? (import tracking)
graphit ast query "MATCH (f:File)-[:IMPORTS]->(m:Module {name: 'express'}) RETURN f.path, f.name" --ai-optimized
```

#### 2. Find Implementors — Who implements an interface/trait?

```bash
# Direct implementors of an interface
graphit ast query "MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Repository'}) RETURN impl.name, label(impl) AS type, impl.path" --ai-optimized

# All implementors of a trait (works for Go interfaces, Java interfaces, Dart abstract classes)
graphit ast query "MATCH (impl)-[:IMPLEMENTS]->(t:Trait {name: 'Serializable'}) RETURN impl.name, impl.path" --ai-optimized

# Find parent interface usages — who calls methods of ANY implementor of interface X?
# (useful for refactoring: if you change the interface, who is affected?)
graphit ast query "MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Handler'}) MATCH (caller)-[:CALLS]->(m:Function) WHERE m.path = impl.path RETURN caller.name, caller.path, impl.name AS implementor" --ai-optimized

# Interface + all concrete method dispatch (receiver_type tracking)
graphit ast query "MATCH (caller)-[r:CALLS]->(method:Function) WHERE r.receiver_type CONTAINS 'Service' RETURN caller.name, method.name, r.receiver_type, caller.path" --ai-optimized
```

#### 3. Call Hierarchy — Upstream and Downstream

```bash
# Outgoing calls — what does this function call? (call tree downward)
graphit ast query "MATCH (f:Function {name: 'HandleRequest'})-[:CALLS]->(callee) RETURN callee.name, label(callee) AS type" --ai-optimized

# Incoming calls — who calls this function? (call tree upward)
graphit ast query "MATCH (caller)-[:CALLS]->(f:Function {name: 'SaveOrder'}) RETURN caller.name, label(caller) AS type, caller.path" --ai-optimized

# Full bidirectional call context (called by + calls)
graphit ast query "MATCH (caller)-[:CALLS]->(f:Function {name: 'ProcessItem'}) RETURN 'called_by' AS direction, caller.name, caller.path UNION ALL MATCH (f:Function {name: 'ProcessItem'})-[:CALLS]->(callee) RETURN 'calls' AS direction, callee.name, callee.path" --ai-optimized

# Method call chain — trace self/this method calls through receiver types
graphit ast query "MATCH (a:Function)-[r:CALLS]->(b:Function) WHERE r.receiver_type = 'OrderService' RETURN a.name AS caller, b.name AS method, a.path" --ai-optimized
```

#### 4. Inheritance & Type Hierarchy

```bash
# Direct subclasses of a class
graphit ast query "MATCH (child:Class)-[:INHERITS]->(parent:Class {name: 'BaseController'}) RETURN child.name, child.path" --ai-optimized

# Full inheritance chain (transitive — all ancestors)
graphit ast query "MATCH (c:Class {name: 'AdminController'})-[:INHERITS*]->(ancestor:Class) RETURN ancestor.name, ancestor.path" --ai-optimized

# Full inheritance tree (transitive — all descendants of a base class)
graphit ast query "MATCH (descendant:Class)-[:INHERITS*]->(base:Class {name: 'AbstractEntity'}) RETURN descendant.name, descendant.path" --ai-optimized

# Combined: class hierarchy + interface implementations
graphit ast query "MATCH (c:Class {name: 'UserRepository'})-[r]->(target) WHERE type(r) IN ['INHERITS', 'IMPLEMENTS'] RETURN type(r) AS relation, target.name, label(target) AS target_type" --ai-optimized
```

#### 5. Containment — File/Class/Package Structure

```bash
# All entities defined in a file (file skeleton)
graphit ast query "MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path ENDS WITH 'service.go' RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number" --ai-optimized

# All methods of a class
graphit ast query "MATCH (c:Class {name: 'OrderService'})-[:CONTAINS]->(m:Function) RETURN m.name, m.line_number, m.is_exported" --ai-optimized

# All fields/properties of a class
graphit ast query "MATCH (c:Class {name: 'User'})-[:HAS_FIELD]->(f:Field) RETURN f.name, f.value" --ai-optimized

# All classes in a directory/module
graphit ast query "MATCH (f:File)-[:CONTAINS]->(c:Class) WHERE f.path STARTS WITH 'src/services/' RETURN c.name, f.path" --ai-optimized

# Package/namespace structure
graphit ast query "MATCH (p:Package)-[:CONTAINS]->(e) RETURN p.name AS package, label(e) AS type, e.name" --ai-optimized
```

#### 6. Field & Property Access Tracking

```bash
# Who reads a specific field?
graphit ast query "MATCH (reader)-[:READS_FIELD]->(f:Field {name: 'balance'}) RETURN reader.name, label(reader) AS type, reader.path" --ai-optimized

# Who writes/modifies a specific field?
graphit ast query "MATCH (writer)-[:WRITES_FIELD]->(f:Field {name: 'status'}) RETURN writer.name, label(writer) AS type, writer.path" --ai-optimized

# All field access (read + write) for a given entity
graphit ast query "MATCH (accessor)-[r]->(f:Field {name: 'email'}) WHERE type(r) IN ['READS_FIELD', 'WRITES_FIELD'] RETURN accessor.name, type(r) AS access_type, accessor.path" --ai-optimized
```

#### 7. DML & Database Dependency Tracking

```bash
# What tables does a procedure/function read from?
graphit ast query "MATCH (p:Procedure {name: 'getCustomerOrders'})-[:SELECTS]->(t:Table) RETURN t.name" --ai-optimized

# What procedures INSERT into a specific table? (write impact)
graphit ast query "MATCH (writer)-[:INSERTS]->(t:Table {name: 'audit_log'}) RETURN writer.name, label(writer) AS type, writer.path" --ai-optimized

# Full DML dependency map for a table (who reads, writes, updates, deletes?)
graphit ast query "MATCH (entity)-[r]->(t:Table {name: 'orders'}) WHERE type(r) IN ['SELECTS', 'INSERTS', 'UPDATES', 'DELETES'] RETURN entity.name, type(r) AS operation, label(entity) AS entity_type" --ai-optimized

# DDL impact — who creates/alters/drops this table?
graphit ast query "MATCH (entity)-[r]->(t:Table {name: 'users'}) WHERE type(r) IN ['CREATES', 'ALTERS', 'DROPS'] RETURN entity.name, type(r) AS ddl_op, entity.path" --ai-optimized
```

#### 8. Refactoring Impact Analysis

```bash
# COMPLETE impact of renaming/changing a function — all inbound edges
graphit ast query "MATCH (dependent)-[r]->(target:Function {name: 'calculateTotal'}) RETURN dependent.name, label(dependent) AS dep_type, type(r) AS relation, dependent.path" --ai-optimized

# COMPLETE impact of changing an interface — implementors + callers of implementors
graphit ast query "MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) RETURN impl.name, impl.path UNION ALL MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) MATCH (caller)-[:CALLS]->(m:Function) WHERE m.path = impl.path RETURN caller.name AS name, caller.path AS path" --ai-optimized

# Safe-to-delete check — is this function called anywhere?
graphit ast query "MATCH (f:Function {name: 'legacyHelper'}) OPTIONAL MATCH (caller)-[:CALLS]->(f) RETURN f.name, f.path, count(caller) AS caller_count" --ai-optimized

# Move-file impact — all entities in a file and their external dependents
graphit ast query "MATCH (f:File {path: 'src/utils/helpers.go'})-[:CONTAINS]->(entity) OPTIONAL MATCH (external)-[r]->(entity) WHERE external.path <> 'src/utils/helpers.go' RETURN entity.name, label(entity) AS type, count(external) AS external_deps" --ai-optimized
```

#### 9. Cross-Cutting Queries

```bash
# Find all entry points (handlers, main functions, test functions)
graphit ast query "MATCH (f:Function) WHERE f.entry_point_score > 50 RETURN f.name, f.entry_point_score, f.path ORDER BY f.entry_point_score DESC" --ai-optimized

# Find all functions with high complexity (candidates for refactoring)
graphit ast query "MATCH (f:Function) WHERE f.cyclomatic_complexity > 15 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC" --ai-optimized

# Find orphan functions (never called — dead code candidates)
graphit ast query "MATCH (f:Function) WHERE NOT ()-[:CALLS]->(f) AND f.entry_point_score < 10 RETURN f.name, f.path" --ai-optimized

# Cross-language dependencies (e.g., Go calling a function defined in SQL)
graphit ast query "MATCH (caller)-[:CALLS]->(callee) WHERE caller.lang <> callee.lang RETURN caller.name, caller.lang, callee.name, callee.lang, caller.path" --ai-optimized

# Find circular dependencies between files
graphit ast query "MATCH (a:File)-[:IMPORTS]->(m1:Module)<-[:CONTAINS]-(b:File)-[:IMPORTS]->(m2:Module)<-[:CONTAINS]-(a) WHERE a.path < b.path RETURN a.path, b.path" --ai-optimized

# Annotation/decorator usage — find all entities with a specific annotation
graphit ast query "MATCH (a:Annotation {name: 'Deprecated'})<-[:CONTAINS]-(owner) RETURN label(owner) AS type, owner.name, owner.path" --ai-optimized

# Parameter analysis — what parameters does a function expect?
graphit ast query "MATCH (f:Function {name: 'createUser'})-[:HAS_PARAMETER]->(p:Parameter) RETURN p.name, p.value, p.line_number" --ai-optimized
```

### Phase 4: Source Code Extraction

The AST graph stores, optionally, the **complete source code** of every indexed file in the
`source` property of `:File` nodes. This is useful when you **discovered** a file
through an AST query and want its content in the same round-trip.

> **⚠️ IMPORTANT: If you already know the file path** and just need to read its content,
> **use your native IDE file-reading tools** (e.g., `view_file`). They are faster and simpler.
> The AST `f.source` pattern is for **discovery workflows** where you found a file through
> structural queries and want metadata + source in a single query.

**Imported ast contexts** also may contain **source code** included in their imported graph,
try query for it to understand the imported code and the overall behaviour of the external contexts.

#### 4a. Get the entire source of a file

```bash
# Retrieve the complete source code of a specific file
graphit ast query "MATCH (f:File {path: 'internal/auth/handler.go'}) RETURN f.source" --ai-optimized
```

#### 4b. Get source of a specific function/class (using line range from the graph)

Use a **two-step pattern**: first find the entity's location, then extract its source from the File node.

```bash
# Step 1: Find the function location
graphit ast query "MATCH (f:Function {name: 'HandleLogin'}) RETURN f.path, f.line_number, f.end_line" --ai-optimized

# Step 2: Get the file source and extract the relevant lines
graphit ast query "MATCH (f:File {path: 'internal/auth/handler.go'}) RETURN f.source" --ai-optimized
```

#### 4c. Get source of all functions in a file with their line ranges

```bash
# List all functions with their line ranges, then fetch the file source to correlate
graphit ast query "MATCH (file:File {path: 'src/services/payment.go'})-[:CONTAINS]->(fn:Function) RETURN fn.name, fn.line_number, fn.end_line ORDER BY fn.line_number" --ai-optimized

# Then get the full file source to slice out individual function bodies
graphit ast query "MATCH (f:File {path: 'src/services/payment.go'}) RETURN f.source" --ai-optimized
```

#### 4d. Quick source peek — verify a function's implementation

```bash
# One-shot: get function metadata + full file source for immediate code review
graphit ast query "MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN fn.name, fn.line_number, fn.end_line, file.path, file.source" --ai-optimized
```

> **When to use file-reading tools instead:** If you already know the file path,
> use your native IDE file-reading tools directly — they are faster and don't consume
> graph query resources. Use `f.source` only when combining discovery + source in one shot,
> or when the `source` property output is too large and you need a specific line range.

## Cypher Guidelines

- **IMPORTANT**: The `path` property is ALWAYS a relative path from the project root (e.g., `src/main.go`). Never use absolute paths when filtering `n.path`.
- **Node type is a LABEL, not a property.** Use `label(n)` — see Phase 1 for details.
- **Property names are exact.** Refer to the Property Reference table in Phase 1. NEVER guess property names — if it's not in the table, it doesn't exist.
- **Shared properties only.** When matching unlabeled nodes (e.g., `MATCH (n) WHERE ...`), you may ONLY access properties shared by ALL labels: `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`. For label-specific properties (e.g., `cyclomatic_complexity`, `is_exported`, `source`), you MUST specify the label in the MATCH (e.g., `(n:Function)`, `(f:File)`).
- LadybugDB strict typing: DO NOT access properties unless you explicitly MATCH the label that contains them. If a property is not shared by ALL possible labels in a pattern, LadybugDB will crash!
- Return only what you need. Avoid returning entire node objects (`RETURN n`); instead, return specific properties (`RETURN n.name, n.path`) to keep output concise.
- Use `LIMIT` only when you want to cap results — don't add it by default.
- Do NOT use the `--context` flag when working with the current project. Only use `--context <name>` when querying an imported third-party AST from the Hub.

## Cluster Filtering

When indexing with `--cluster <name>`, all nodes are tagged with a `cluster` property.
This allows you to logically group indexations and query them selectively.

### Indexing with a cluster tag:
```bash
graphit ast index ./src --cluster backend
graphit ast index ./legacy --parser plsql --cluster erp-core
```

### Querying by cluster:
```bash
graphit ast query "MATCH (n:Function {cluster: 'backend'}) RETURN n.name, n.path" --ai-optimized
graphit ast query "MATCH (n:Class {cluster: 'erp-core'}) RETURN n.name" --ai-optimized
graphit ast query "MATCH (n {cluster: 'my-module'}) RETURN label(n), n.name" --ai-optimized
```

The `cluster` property is indexed on all node labels for fast lookups.

## 🔄 Fallback to Built-In Tools — ONLY for What the Graph Does Not Contain

**Your built-in tools (grep, ripgrep, semantic search, code symbols) are permitted
ONLY for information that the AST graph structurally cannot contain.**
The graph is ALWAYS your primary source. It is NOT a "first attempt" — it is
the definitive structural model. Your tools exist only for non-structural queries.

Your tools are allowed ONLY when ALL of these conditions are true:

1. You **already queried the AST graph** for the information
2. The graph **genuinely cannot answer** (e.g., string literal content, comment text, runtime values)
3. You **state explicitly** to the user: "The AST graph cannot answer X, falling back to text search"

**If even ONE of these conditions is not met, you MUST NOT use your tools.**

Examples of valid fallback:
- Searching for a specific error message string → grep source code (graph doesn't index string literals)
- Finding TODO comments → grep for `// TODO` (graph has optional docstring, not all comments)
- Checking environment variable usage → grep for `process.env.X` (runtime values, not structural)

Examples of INVALID fallback (protocol violations):
- Grepping for function definitions instead of querying the graph → ❌ graph has all functions indexed
- Using code symbols to find class hierarchy → ❌ graph has INHERITS edges with transitive closure
- Reading files to understand what a module exports → ❌ graph has CONTAINS edges from File to entities
- Searching for "who calls X" via grep → ❌ graph has CALLS edges between functions

### ❌ Anti-patterns (violations of this protocol)

| Anti-pattern | Why it is a violation |
|---|---|
| `grep -r "functionName" src/` | Brute-force; graph has O(1) indexed lookup for all entity names |
| Using IDE go-to-definition | Graph works cross-file, cross-language, cross-context |
| Reading files to understand structure | Graph gives instant file skeleton via CONTAINS edges |
| Semantic search for "who calls X" | Graph has exact CALLS edges — no guessing needed |
| Manually tracing imports file by file | Graph has pre-resolved IMPORTS edges |
| Counting branches to estimate complexity | Graph has pre-computed `complexity` property |
| Assuming you know all usages | Graph knows ALL callers, importers, inheritors — your memory doesn't |

## ⚡ MANDATORY: Background Reindex After Every File Modification

**After ANY modification to ANY source code file (edit, create, rename, or delete),
you MUST trigger an incremental sync in the background:**

```bash
graphit sync &
```

**This is NON-NEGOTIABLE.** The framework depends on an up-to-date index to function.
Without reindexing, the AST graph becomes stale and subsequent queries return
outdated or incomplete results — breaking the analysis pipeline.

**Rules:**
- Run **fire-and-forget** — do NOT wait for it to finish. Continue your work immediately.
- Run it after **every** file modification, not just at the end of a task.
- If you modified multiple files, a single index call at the end covers all changes.
- The AST graph will reflect your changes the next time it is queried.
- **Forgetting to reindex is a framework integrity violation.**

