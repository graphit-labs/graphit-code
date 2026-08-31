---
name: graphit-ast
description: 'AST Code Exploration and structural analysis via graph database. Your PRIMARY and FIRST code analysis tool — use INSTEAD OF grep_search, ripgrep, or file-by-file reading for all structural queries. Use for: finding functions, classes, methods, callers, imports, inheritance, unused code, refactoring impact, entry points, cyclomatic complexity, DML/database dependencies, file structure, and module relationships. Also use when spawning subagents that need to explore code.'
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

> **Access it via the AST MCP tools ONLY — NEVER via the CLI.** These MCP tools
> take ABSOLUTE PRECEDENCE over your native/built-in code tools whenever a
> structural query is possible.

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
| **Before editing a function, method, or entity — ANY edit, not only a planned refactor** | Query its callers, its callees, and whether a test reaches it — see the Pre-Edit Impact Check below | ❌ Don't edit based on "I read it and it looks self-contained" |
| **Understanding file structure** | `MATCH (f:File {path: 'X'})-[:CONTAINS]->(e) RETURN label(e), e.name, e.line_number` | ❌ Don't read the entire file to understand its structure |
| **Finding DML dependencies** | `MATCH (p:Procedure)-[:SELECTS]->(t:Table) RETURN p.name, t.name` | ❌ Don't grep for `SELECT.*FROM` in SQL files |
| **Checking complexity** | `MATCH (f:Function) WHERE f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity` | ❌ Don't manually count branches in source code |
| **Finding unused code** | Collect called names, then exclude declarations: `MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called RETURN f.name, f.path` | ❌ Don't grep for function names — and prefer this name-based form over `NOT ()-[:CALLS]->(f)`, which also counts the calls that stayed on a stub |
| **Refactoring impact analysis** | Query all inbound edges (CALLS, INHERITS, IMPORTS) to the target entity | ❌ Don't assume you know all usages from reading a few files |
| **Finding entry points** | `MATCH (f:Function) WHERE f.is_exported AND (f.name = 'main' OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler') RETURN f.name, f.path` — adjust the name/decorator pattern to the framework actually in use, which you already know | ❌ Don't invent a scoring property — the graph has no precomputed entry-point score; reason about the framework's own conventions |
| **Tracing self/this calls** | `MATCH (a)-[r:CALLS]->(b) WHERE r.receiver_type <> '' RETURN a.name, b.name, r.receiver_type` | ❌ Don't read source to trace method dispatch — graph has receiver_type on CALLS edges |
| **Finding interface implementors** | `MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface {name: 'X'}) RETURN c.name` | ❌ Don't grep for `implements X` — graph has IMPLEMENTS edges |

## 🔒 MANDATORY: Before You Edit — the Pre-Edit Impact Check

**Before you call your edit tool on any function, method, class, struct, interface, or other
entity — not only for a deliberate refactor, but for a change that looks like a one-liner —
query the graph first.** "I read the function and it looks self-contained" is an opinion about
text you happened to see; the questions below have actual answers, and only the graph has them.

**1. Who calls it, and who calls them — the blast radius.** Use multi-label matching on the
target if you are not certain of its exact label (Function vs Method, Class vs Struct):
```
MATCH (dependent)-[r]->(target) WHERE (label(target) = 'Function' OR label(target) = 'Method') AND target.name = 'X' RETURN dependent.name, label(dependent) AS type, type(r) AS relation, dependent.path
```
Every row is a caller whose behavior may change the moment you change `X`. Zero rows is itself
an answer — it means the entity is unreferenced by anything this graph can see, which is
different from "safe", not a synonym for it: check entry points and exported status before
treating it as dead.

**2. Is it tested — the safety net you would otherwise be editing without.** This graph has no
`is_test` flag and no `TESTS` edge; test coverage is call evidence. A test is a function whose
name matches this project's test convention (`Test*`, `test_*`, `*_test`, `*Test`, depending on
language and framework) that reaches the target via `CALLS`, directly or transitively:
```
MATCH (caller)-[:CALLS*1..3]->(t) WHERE ((label(t) = 'Function' OR label(t) = 'Method') AND t.name = 'X') AND toLower(caller.name) CONTAINS 'test' RETURN DISTINCT caller.name, caller.path
```
**Zero rows here is not "nothing to worry about" — it is the opposite: no test will tell you
if your edit broke this.** Say that explicitly, and either treat the change with the extra care
that implies, or write the missing test before you change the behavior it would have caught.

**3. What it depends on, so the edit does not silently drop something it needs:**
```
MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND f.name = 'X' MATCH (f)-[:CALLS]->(callee) RETURN callee.name, label(callee) AS type, callee.path
```

**4. If it is a type, interface, or field — who else moves with it:**
```
MATCH (impl)-[:IMPLEMENTS]->(i:Interface {name: 'X'}) RETURN impl.name, impl.path
MATCH (child)-[:INHERITS]->(parent:Class {name: 'X'}) RETURN child.name, child.path
MATCH (accessor)-[r]->(f:Field {name: 'X'}) WHERE type(r) IN ['READS_FIELD', 'WRITES_FIELD'] RETURN accessor.name, type(r) AS access_type, accessor.path
```

**This check is not one-and-done for the session.** A follow-up request to touch a *different*
function five turns later needs its own four answers — familiarity with the file from your last
edit is not familiarity with this entity's callers, and assuming otherwise is exactly how a
change that looked complete breaks a caller you never queried.

### When you should NOT use the AST graph

| Scenario | Use instead |
|---|---|
| Editing source code | File edit tools — the graph is read-only |
| Running tests or builds | Terminal commands |
| Checking runtime behaviour or logs | Terminal/browser tools |
| Understanding project documentation | Knowledge wiki (not AST) |

### Two cases that look like exceptions and are not

**"I already know the path, I will just read the file."** Use `graphit_ast_source`.
It reads the indexed copy, so one call gives you a line range, a single function by name, or
a pattern with context — where a plain file read gives you the whole file and you pay for
every line of it in tokens. It is also the only one of the two that works on an imported
context, whose files are not in this checkout at all.

Your native read is correct in one situation, and it is worth naming: the file is **not in
the graph** — you just created it, or `.astignore` excludes it, or `ast.index_source` is
`false` so the graph holds structure without text. `graphit_ast_source` says
so plainly when that happens; that answer is your signal to read from disk, not a reason to
skip the tool.

**The documentation tree is the one exclusion you will meet most often, and it is not a
missing index.** This graph does not hold `knowledge.docs_dir` — `docs/` by default —
because the knowledge wiki owns it and chunks, links and ranks prose in ways a code graph
cannot. A query that finds no `File` node under `docs/` is telling you to ask the wiki, not
to fall back to grep: `graphit_knowledge_search`, `graphit_wiki_browse`, `graphit_wiki_source`.
A project that genuinely wants structural queries over files kept there — `.proto` or
`.graphql` schemas, say — sets `ast.index_docs` to `true`; a `!docs/` line in `.astignore`
will not do it, because built-in defaults are applied last and outrank the project's own
patterns.

**A whole language can be switched off, and that is configuration too — not a broken
index.** `ast.grammars_blacklist` disables the grammars it names, and
`ast.grammars_whitelist`, when non-empty, disables every grammar it does NOT name. Both
are inherited from the machine's global config, so the reason a language is absent may
not be in this project at all. When the graph holds no node whatsoever for a language
that is plainly in the repository, read those two keys with `graphit_config_get` before
concluding the index is stale or falling back to grep — and note that a name matching no
known grammar is deliberately inert, so a typo in either key disables nothing and says
nothing.

**A SQL corpus with an empty graph is configuration, not a broken index.** The SQL
dialect grammars — `plsql`, `postgresql`, `db2`, `tsql`, `plpgsql` — are declared
`exclusive` in their query YAML, which means they claim no extensions: `.sql` is parsed
by the tree-sitter `sql` grammar and nothing falls back to a dialect, while `.pks`,
`.pkb`, `.db2` and `.tsql` are not indexed at all. A project indexes with a dialect by
naming it — `ast.grammar` = `.sql=antlr-plsql,.pks=antlr-plsql`. So when an Oracle or
T-SQL repository has no Procedure and no Package in its graph, read `ast.grammar` with `graphit_config_get`
before reindexing anything.

**"I need to search inside comments."** Comments are **in the graph**, as `Comment` nodes
whose `name` is the comment text:
```
MATCH (c:Comment) WHERE toLower(c.name) CONTAINS toLower('deprecated') RETURN c.name, c.path, c.line_number
```

Which beats grep on the thing grep is supposedly good at: no regex to escape, results already
attached to a file and a line, and a block comment arrives as one node instead of five
unrelated matching lines.

String literals **inside function bodies** genuinely are not entities. Even there, try
`graphit_ast_source` with `pattern` first — it searches the indexed text
with `before`/`after` context, scoped to a file or a single entity. Fall back to
grep/ripgrep across the tree when you do not know which file to look in.

## This Is a Real Graph Database, and Cypher Is the Point

The index is **LadybugDB**, a property-graph database you query with **Cypher** — the same
language as Neo4j: `MATCH` patterns, variable-length paths, aggregation, `UNION`,
`OPTIONAL MATCH`. `graphit_ast_query` runs arbitrary Cypher against it.

**This is the most powerful tool you have here, and the one most often left unused.** The
failure mode is specific and worth naming: an agent runs `graphit_ast_search`,
gets a ranked list of plausible entities, and answers from that list. Search is a **text**
tool — it finds *names*. It cannot tell you what calls what, what breaks if you change
something, or what the shape of a subsystem is. Those are traversals, and only a query does
traversals.

### The difference, on the same question

| The question | `graphit_ast_search` gives you | A Cypher query gives you |
|---|---|---|
| "How does authentication work?" | ~15 entities whose text resembles "authentication" | the call chain from the entry point through to the token check, with every hop named |
| "Who uses `saveUser`?" | places where the string `saveUser` appears | every caller, transitively, with its file and line — and nothing that merely mentions the name in a comment |
| "Is this safe to delete?" | nothing about safety | the exact count of inbound edges, which is the answer |
| "What is the riskiest code here?" | nothing — risk is not a word in the source | functions ranked by `cyclomatic_complexity`, and which of them have no callers |
| "What does this file contain?" | maybe some of its entities | every declaration and comment in line order |

**So: search is the step that finds names when you do not know them. It is never the last
call.** Whatever it returns, the next step is a query using those names — that is the whole
of Phase 2 → Phase 3 below. Answering straight out of a search result is answering from text
similarity, which is the thing this graph exists to replace.

### What a query answers that nothing else can

Five families of question, each with the query that settles it. These are the reason the
graph is here; the cookbook further down expands every one of them.

**1. Relationships between entities** — the graph's native question. Edges are `CALLS`,
`CONTAINS`, `IMPORTS`, `INHERITS`, `IMPLEMENTS`, `HAS_FIELD`, `HAS_PARAMETER`,
`READS_FIELD`, `WRITES_FIELD`, and the SQL DML/DDL set.
```bash
# what does this function actually depend on, one hop out
MATCH (f:Function {name: 'ProcessOrder'})-[:CALLS]->(callee) RETURN callee.name, label(callee) AS type, callee.path

# and the reverse: what depends on it
MATCH (caller)-[:CALLS]->(f:Function {name: 'ProcessOrder'}) RETURN caller.name, caller.path
```

**2. Find usage — the real one, not a text match.** Text search finds the name in comments,
strings and unrelated identifiers. The graph finds *call edges*.
```bash
# direct callers
MATCH (caller)-[:CALLS]->(t:Function {name: 'ValidateToken'}) RETURN caller.name, label(caller) AS type, caller.path

# transitive — everything that can reach it within three hops
MATCH (caller)-[:CALLS*1..3]->(t:Function {name: 'ValidateToken'}) RETURN DISTINCT caller.name, caller.path

# usage of a type, by any kind of edge
MATCH (user)-[r]->(c:Class {name: 'UserService'}) RETURN user.name, type(r) AS relationship, user.path
```

**3. Refactoring — the blast radius, before you touch anything.** This is the question you
cannot answer by reading: *what else moves if I change this?*
```bash
# every inbound edge to the thing you are about to change, of every kind
MATCH (dependent)-[r]->(target:Function {name: 'calculateTotal'}) RETURN dependent.name, type(r) AS relation, dependent.path

# safe to delete? the count IS the answer
MATCH (caller)-[:CALLS]->(t:Function {name: 'legacyHelper'}) RETURN count(caller) AS callers

# renaming an interface: implementors first, then who calls into them
MATCH (impl)-[:IMPLEMENTS]->(i:Interface {name: 'PaymentGateway'}) RETURN impl.name, impl.path

# moving a file: which of its entities have dependents outside it
MATCH (f:File {path: 'internal/util/helpers.go'})-[:CONTAINS]->(e) OPTIONAL MATCH (outside)-[r]->(e) WHERE outside.path <> f.path RETURN e.name, count(outside) AS external_dependents ORDER BY external_dependents DESC
```

**4. Complexity and risk — computed at index time, so it is one query away.**
`cyclomatic_complexity` is a property on every callable; you do not count branches by hand.
```bash
# the refactoring backlog, ordered
MATCH (f:Function) WHERE f.cyclomatic_complexity > 15 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# complex AND unreachable — the worst combination, and invisible to any text tool.
# Sanity-check the results yourself against the framework's own entry-point conventions
# (main, test names, route/handler decorators) before calling anything dead.
MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called AND f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# which files concentrate the complexity
MATCH (file:File)-[:CONTAINS]->(f:Function) RETURN file.path, count(f) AS functions, sum(f.cyclomatic_complexity) AS total ORDER BY total DESC
```

**5. Understanding a system you have never read.** Start from the outside and walk inwards —
a few queries replace hours of file-by-file reading, and they cannot miss a path the way
reading does.
```bash
# what languages and how much of each — the shape of the repository
MATCH (f:File) RETURN f.lang, count(f) AS files ORDER BY files DESC

# the doors in: handlers, mains, tests — you already know the framework's conventions,
# so name the patterns yourself instead of looking for a precomputed score
MATCH (f:Function) WHERE f.is_exported AND (f.name = 'main' OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler') RETURN f.name, f.path

# the busiest functions — high fan-in means everything depends on them
MATCH (caller)-[:CALLS]->(f:Function) RETURN f.name, f.path, count(caller) AS callers ORDER BY callers DESC LIMIT 20

# the hubs by fan-out — these are the orchestrators worth reading first
MATCH (f:Function)-[:CALLS]->(callee) RETURN f.name, f.path, count(callee) AS calls ORDER BY calls DESC LIMIT 20

# a module's public surface, without opening a file
MATCH (file:File)-[:CONTAINS]->(e) WHERE file.path STARTS WITH 'internal/auth/' AND e.is_exported AND label(e) IN ['Function', 'Method', 'Struct', 'Class', 'Interface', 'Type', 'Constant'] RETURN file.path, label(e) AS type, e.name ORDER BY file.path

# what this project depends on, ordered by how much of it depends on each
MATCH (file:File)-[:IMPORTS]->(m:Module) RETURN m.name, count(file) AS imported_by ORDER BY imported_by DESC LIMIT 30
```

Note what these have in common: **`count`, `sum`, `ORDER BY` and `DISTINCT` work.** You are
not limited to lookups — you can ask the graph to rank, aggregate and compare, which is how
one query answers a question that would otherwise be a survey. `label(n) IN [...]`,
`STARTS WITH`, `CONTAINS`, `OPTIONAL MATCH` and `UNION ALL` all work too.

> ⚠️ **`is_exported` is set on `Comment` nodes as well as declarations.** Filtering by it alone
> returns the file's comments along with its API — which is why the query above also pins the
> labels with `label(e) IN [...]`. Whenever a filter is about *declarations*, say which labels
> you mean; the graph holds more kinds of node than the property name suggests.

## Phased Graph Exploration

The phases below are how you get from a vague request to a precise query. Phase 2 finds the
names; **Phase 3 is where the question actually gets answered.**

### Phase 1: Know the schema — call the schema tool BEFORE your first query

🔒 **MANDATORY: the first AST call you make is `graphit_ast_schema`, not a query.**
Before you write Cypher — not after it fails:
```
graphit_ast_schema(project_dir: "/path/to/project")
```
It returns **every node label with its complete property list**, and every relationship type
with its edge properties. That output is the authority; the tables below are a summary of the
labels you meet most often, and a summary is not a schema. Call it again whenever you are
about to reach for a property you have not used yet in this session, and **always** when you
switch to a different `project_dir` or an imported `context` — labels are per-project: a
repository with no SQL has no `Table`, one with stylesheets has a dozen labels these tables
never mention.

**Writing a query from what a property name *ought* to be is the most common way this module
fails.** It does not degrade gracefully — it crashes, and an agent that guessed once tends to
guess again. Two real failures from one session, back to back:
```
❌ MATCH (n) WHERE n.path CONTAINS 'internal/hub/' RETURN n.type, n.name, n.path, n.line
   → Binder exception: Cannot find property type for n

❌ MATCH (n:Function) WHERE toLower(n.name) CONTAINS 'event' RETURN n.name, n.path, n.line
   → Binder exception: Cannot find property line for n
```
Neither `type` nor `line` exists: the node type is `label(n)`, and the line is `line_number`.
One schema call up front would have cost less than either failure.

#### Properties that do NOT exist — and what to write instead

Every entry in the first column is something an agent has plausibly reached for. None of them
bind.

| You are about to write | Why it fails | Write this instead |
|---|---|---|
| `n.type`, `n.kind`, `n.label`, `n.node_type` | the node type is a **label**, not a property | `label(n) AS type` |
| `n.line`, `n.lineno`, `n.start_line`, `n.row` | invented name | `n.line_number` |
| `n.end`, `n.end_line_number` | invented name | `n.end_line` |
| `n.file`, `n.filename`, `n.filepath`, `n.file_path` | invented name | `n.path` — always relative to the project root |
| `n.complexity` | invented name | `n.cyclomatic_complexity` |
| `n.doc`, `n.comment`, `n.comments` | invented name | `n.docstring` — or match a `Comment` node, whose `name` IS the comment text |
| `n.is_public`, `n.visibility`, `n.public` | invented name | `n.is_exported` |
| `n.body`, `n.code`, `n.text`, `n.content` | source text is not on entity nodes | `graphit_ast_source` |
| `n.params`, `n.args`, `n.signature`, `n.arity` | parameters are nodes, not a property | `MATCH (n)-[:HAS_PARAMETER]->(p:Parameter) RETURN p.name` |
| `n.returns`, `n.return_type` | not indexed | read the declaration with `graphit_ast_source` |
| `n.is_test`, `n.is_dead`, `n.is_used` | no such flags exist | name convention + `CALLS` evidence — see the Pre-Edit Impact Check |
| `n.package`, `n.module`, `n.namespace` | invented name | `n.context` / `n.class_context`, or the containing `File.path` |
| `n.callers`, `n.callees`, `n.dependencies` | edges are not properties | `MATCH (caller)-[:CALLS]->(n) RETURN count(caller)` |
| `r.line`, `r.file` on a relationship | edges have their own property set | `r.line_number`, `r.source_file` |

#### `Binder exception: Cannot find property X for n` — the recovery protocol

The error names the exact property it could not bind. **Do not guess a second name — call
`graphit_ast_schema` and read that label's row, then rewrite once.** A guessing
loop burns four calls and still lands on the wrong query.

There are three distinct causes, and the third is the one that will send you off to fix a
query that was already correct:
1. **The property does not exist at all** (`type`, `line`, `complexity`) — the table above has
   the real name.
2. **It exists, but not on any label this pattern can reach.** A property binds on an
   unlabelled `MATCH (n)` when **any** candidate label has it — so `n.line_number` and even
   `n.is_exported` are fine on a complete graph. It fails when the pattern is pinned to labels
   that lack it: `MATCH (n:Function) RETURN n.relative_path` crashes, because `relative_path`
   is a `File` property. Pin the right label — and note that `WHERE label(n) IN [...]` does
   **not** rescue the binding, because binding happens before filtering.
3. **The graph is mid-rebuild, and the property is momentarily on no table at all.** During a
   reindex the schema is partial — `File`, `Directory` and a couple of stub tables — so a
   property every entity normally carries binds nowhere, and a correct query is answered as
   though it were wrong. **This looks exactly like cause 1.** The tell is in the error itself:
   the message names the tables the graph currently holds, and a handful of them where you
   expect thirty is a rebuild, not a typo. Retry before rewriting anything; `graphit_ast_schema` and `graphit_daemon_status` settle it.

#### `canonical catalog: ...` — a refusal, and it names the rule

A mounted/remote graph — a Hub context, an imported bundle — stores each label as its own
table, so a traversal over a LOGICAL relationship type (`CALLS`, `CONTAINS`) is answered by
a dedicated planner, and anything it cannot preserve exactly fails closed rather than
running a plan that would enumerate the whole component.

**The message names the rule you broke and the query that works — read it and rewrite once.**
The two you will meet:
1. **`a label is not projectable here`** — the label IS the table, so a traversal over a
   logical type has no label column. Pin the label in the PATTERN and run one query per
   label: `MATCH (f:File)-[:CONTAINS]->(e:Function) ... RETURN DISTINCT e.name`, not
   `RETURN DISTINCT label(e)`.
2. **`a projection over a traversal must be DISTINCT`** — add `DISTINCT`. The planner
   answers with the SET of reached nodes, not one row per path.

Also refused, each saying so: a RETURN projecting both ends or neither, a projection richer
than `endpoint.property`, a predicate comparing the two ends, and a traversal with nothing
filtering the end it starts from. A node-only query is unaffected — `label(n)` is fine there;
the restriction is about traversing a logical type.

#### Property Reference (the common labels — the schema tool is still the authority)

| Node Type | Properties |
|---|---|
| **File** | `path`, `name`, `relative_path`, `is_dependency`, `lang`, `cluster` |
| **Directory** | `path`, `name`, `cluster` |
| **Function, Method, Class, Struct, Interface, Type, Variable, Constant, Field, Parameter** | `uid`, `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`, `cyclomatic_complexity`, `context`, `context_type`, `class_context`, `is_dependency`, `is_exported`, `value`, `is_stub`, `cluster` |
| **Module** | `uid`, `name`, `lang`, `full_import_name`, `path`, `line_number`, `end_line`, `docstring`, `cyclomatic_complexity`, `context`, `context_type`, `is_dependency`, `is_exported`, `is_stub`, `cluster` |
| **Comment** | `uid`, `name` — **`name` is the comment text itself** — plus `path`, `line_number`, `end_line`, `lang`, `cluster` |

> `File` has no `source` column — file text lives in the search index, the only copy that
> is actually queryable. Source code comes from `graphit_ast_source`, never from a `RETURN f.source`.

#### 🔒 `CALLS` points at the real declaration — except when there is none

A call is joined to the entity that declares it whenever the name resolves to exactly one
declaration, so `CALLS` normally lands on the same node that `CONTAINS` reaches from the file,
with its real `path` and `line_number`. Traversals compose: you can walk from a caller to its
callee's file, or count inbound edges on a declaration, and both work.

**A stub remains only when there is nothing in this graph to point at**, and there are exactly
two such cases:

| `is_stub` | what it is | `path`, `line_number` |
|---|---|---|
| `false` | the declaration — the normal target of a call | the real ones |
| `true` | the name resolves to **nothing** here (`len`, `fmt.Errorf` — a builtin or a dependency), or to **several** declarations, so joining it would invent an edge nobody wrote | **empty and `0`** |

A stub carries the `lang` of the file that called it, so it still groups under a language — it
is not a node of unknown origin, it is a known call to something outside the corpus.

**What this means for your queries:**

1. **Inbound `CALLS` on a declaration is meaningful.** `MATCH (caller)-[:CALLS]->(f:Function {name: 'X'}) RETURN count(caller)`
   answers how many callers `X` has. An ambiguous or external name is the case where it will
   not, and `is_stub` is how you tell.
2. **Filtering a call target by `path` still needs care.** A stub has none, so add
   `f.is_stub = false` to any query that RETURNS `path` or `line_number`, or you will report an
   empty path and line `0` as if it were a real location.
3. **A method is a legitimate call target.** `CALLS` ends at `Method` as readily as at
   `Function`, so pin both when you match by label: `label(b) = 'Function' OR label(b) = 'Method'`.

```bash
# who calls X — now a direct traversal, including through to the callee's file
MATCH (caller)-[:CALLS]->(t) WHERE (label(t) = 'Function' OR label(t) = 'Method') AND t.name = 'Apply' RETURN caller.name, caller.path

# uncalled declarations — dead code candidates. Name-based comparison still catches the
# unresolved remainder, so it stays the safer form; cross-check survivors against the
# framework's entry-point conventions before calling anything dead.
MATCH ()-[:CALLS]->(s) WITH collect(DISTINCT s.name) AS called MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called RETURN f.name, f.path, f.cyclomatic_complexity ORDER BY f.cyclomatic_complexity DESC

# what is called here but declared nowhere in this corpus — the external surface
MATCH (f:Function) WHERE f.is_stub = true RETURN f.name, f.lang ORDER BY f.name
```

> **`Comment` is why you do not need grep for comments.** Every comment is an entity, not just
> the ones attached to a declaration: licence headers, notes inside a function body, an
> explanatory block between two functions, commented-out code. A multi-line comment is **one**
> node spanning `line_number`..`end_line`, so you match the whole thought instead of the one
> line that happened to contain your keyword.

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

> 🔑 **EXPECT MORE THAN ONE NODE PER NAME — most of the time that is CORRECT, not a bug.**
> Four labels are named after their **content** instead of after an identifier:
> **`Value`**, **`AttributeValue`**, **`Text`** and **`Comment`**. Their `name` IS the literal,
> the attribute value, the character data, the comment text. So a name lookup returns them
> beside the real declarations, and the two look alike in the result.
>
> The case you WILL meet: a procedure `PRC_X` that assigns its own name to a variable —
> `v_prog_name := 'PRC_X';` — produces a `Value` node named `PRC_X`, sitting next to the
> `Procedure` named `PRC_X`. Both are right. One is the declaration; the other is a mention
> of it. Measured on a real corpus: 338 of its procedures have a `Value` twin this way.
>
> **So `label(n)` is not decoration in that query — it is the column that answers which row
> is the declaration.** Read it, and pin the label as soon as you know which one you meant:
> `MATCH (n:Procedure) WHERE n.name = 'PRC_X' ...`. Do not report two nodes as a duplicate,
> a leak, or a corrupt index, and do not pick the first row because it came first.
>
> This does NOT mean the graph's edges are polluted: a relation resolves its target through
> the grammar's `target_rules`, which name the labels a relation may mean — `CALLS` in the
> SQL family allows `[Function, Procedure, Package, Trigger]`, so a `Value` is never a
> candidate. It affects what a bare discovery query SHOWS YOU, not what the graph joined.

> ⚠️ **CRITICAL: The node type is its **graph label** (e.g., `Function`, `Class`, `Method`).
> To get the node type, use the built-in **`label(n)`** function, NOT a property access.
> Writing `RETURN n.kind` or `RETURN n.type` will crash with `Cannot find property`.
> ✅ Correct: `RETURN label(n) AS type`
> ❌ Wrong: `RETURN n.kind` / `RETURN n.type` / `RETURN n.label`

### Phase 2.3: Hybrid Search — the best way to FIND NAMES, never the answer

**This is the recommended way to turn a vague request into entity names**, and that is
precisely the whole of its job. It combines BM25 full-text search with semantic vector search
using Reciprocal Rank Fusion (RRF, k=60) into one ranking — call the `graphit_ast_search` tool (passing absolute `project_dir` and `query`):
```
graphit_ast_search(project_dir: "/path/to/project", query: "authentication and session management")
```

> 🔒 **Its output is input for Phase 3, not a result to report.**
> A search result is a ranked guess from text similarity. It does not know what calls what,
> what would break, or how complex anything is — it never traversed an edge. Take the names
> it found and query them. Two calls, and the second is the one that answers the question.
>
> Stopping at the search result is the single most common way this graph goes unused: the
> answer *looks* grounded, because it names real entities, while the relationships it implies
> were never checked.

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
If semantic results are empty, call `graphit_ast_embed` (passing `project_dir`) — fire-and-forget, do not wait for it to finish.
Not `graphit_sync`: that also reindexes the AST graph, both wikis and the Hub to fix one missing set of vectors.
In hybrid mode, search gracefully falls back to FTS-only while the vectors are missing, so an empty *semantic* result does not mean an empty *hybrid* result — retry in hybrid before concluding the code is not there.

### Phase 3: Precise Graph Query — where the question gets answered

**Phase 2 gave you names. This is the phase that produces the answer**, and it is not
optional: every request that is about relationships, impact, complexity or structure ends
here. If you found yourself writing a reply straight after Phase 2, go back and query.

Once you know the exact names and labels from Phase 2, construct the final query. Call the `graphit_ast_query` tool (passing `project_dir` and `query`):

> 🔒 **If you are exploring — the question is still open, or the answer's boundary is not yet
> clear — pair this query with a hybrid search (`graphit_ast_search`) on the
> same topic.** A Cypher `MATCH` is exact and narrow by design — it returns only what the
> pattern names, and a narrow pattern can quietly under-cover a broad question. A hybrid
> search on the same words surfaces related entities, near-duplicates, and comments the
> query's exact shape does not ask for, giving you the larger, more complete picture.
>
> This does **not** apply once you are certain of the specific query you want — a targeted,
> deterministic check ("how many callers does `X` have", "does `Y` implement `Z`") needs only
> its own precise answer, and a hybrid search alongside it adds noise, not context.
>
> **Nor does it apply when the corpus has no prose and a rigid naming convention.** Both
> halves of a hybrid search rank on text: BM25 over names and comments, and vectors over
> what that text means. A schema-and-stored-procedure repository where every object is
> named by a fixed prefix (`PRC_`, `PCK_`, `IX_`) gives both halves nothing to separate one
> candidate from another — the observed result is fifteen near-identical scores around
> 0.03–0.05, in an order that carries no signal. The convention already tells you the name,
> so go straight to Cypher and use `STARTS WITH` on the prefix. Read one search result set;
> if the scores are flat and the top hit is not obviously better than the tenth, that is
> this case, and repeating the search with different words will not fix it.

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

> The full-discovery row is deliberately unfiltered, so it also returns the content-named
> labels — `Value`, `AttributeValue`, `Text`, `Comment` — whose `name` is a literal that
> happens to equal the name you asked for. That is the correct answer to "anything called
> X", and the reason the row projects `label(n)`. See Phase 2 before concluding that a
> second node with the same name is a duplicate or a mislabel.

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
MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called RETURN f.name, f.path

# High-complexity functions
MATCH (f:Function) WHERE f.cyclomatic_complexity > 10 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# Entry points — no precomputed score; name the framework's own conventions yourself
MATCH (f:Function) WHERE f.is_exported AND (f.name = 'main' OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler') RETURN f.name, f.path

# Receiver type — trace self/this method calls to their owning class
MATCH (a:Function)-[r:CALLS]->(b:Function) WHERE r.receiver_type IS NOT NULL AND r.receiver_type <> '' RETURN a.name, b.name, r.receiver_type

# Interface implementations — who implements interface X?
MATCH (c:Class)-[:IMPLEMENTS]->(i:Interface {name: 'Handler'}) RETURN c.name, c.path
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

# and WHERE the dependency is pulled in — the statement site, with its line
MATCH (f:File)-[:CONTAINS]->(s) WHERE label(s) IN ['Import', 'Include', 'Export'] AND s.name CONTAINS 'express' RETURN f.path, label(s) AS form, s.name, s.line_number ORDER BY f.path
```

> **A dependency is in the graph twice, and the two halves answer different questions.**
> The `IMPORTS` edge points at a **canonical `Module`** node, so every file pulling in the
> same module points at one node — that is the dependency question, and it is the same
> edge in every language. The **statement** is a separate entity, in one file at one line,
> which the shared module node cannot tell you.

> The statement carries the label of the form the language actually uses, because they
> are not the same statement: **`Import`** for `import x` / `use` / `require`,
> **`Include`** for a C preprocessor `#include`, **`Export`** for a JavaScript
> `export ... from './x'`. All three produce the `IMPORTS` edge. So filter by the label
> when the form matters, and use `label(s) IN ['Import', 'Include', 'Export']` — or just
> the edge — when it does not.

#### 2. Find Implementors — Who implements an interface/trait?

Query templates to run with `graphit_ast_query`:
```bash
# Direct implementors of an interface
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Repository'}) RETURN impl.name, label(impl) AS type, impl.path

# All implementors of a trait (works for Go interfaces, Java interfaces, Dart abstract classes)
MATCH (impl)-[:IMPLEMENTS]->(t:Trait {name: 'Serializable'}) RETURN impl.name, impl.path

# Implementors of an interface, then their callers — two queries, because IMPLEMENTS and CALLS
# cannot be joined through one node. Step 1 gives you paths; step 2 uses the names you found.
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'Handler'}) RETURN impl.name, impl.path

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

> 🔒 **A DML query that returns NOTHING, or only readers, is the one result you must not
> take at face value.** "Nobody writes to this table" is a conclusion with consequences,
> and the failure mode here is not a missing edge — it is an edge that resolved to a
> different node than you matched on. Before concluding absence, ask where the edges of
> that file actually went:
```bash
MATCH (a)-[r]->(b) WHERE r.source_file = 'path/to/file' RETURN type(r), label(a) AS src, label(b) AS dst, count(*) AS n ORDER BY n DESC
```
> `File → File` on a DML type means the target NAME did not resolve to a declaration, so
> it fell back to the file. The usual causes: the table is genuinely not declared in this
> corpus, or the statement was parsed by an embedded grammar (SQL inside XML, inside a
> string) whose declarations live under another language. Either way the edge exists and
> your `:Table` pattern could not see it.

> **"Is this rule enforced by the database, or only by the application?"** — an index
> carries what answers that, so answer it from the graph rather than from the DDL:
```bash
MATCH (i:Index)-[:REFERENCES]->(t:Table {name: 'PEDIDO_ITEM'}) RETURN i.name, i.value AS unique_marker
MATCH (i:Index {name: 'IU_PEDIDO_PROD'})-[:CONTAINS]->(c:Column) RETURN c.name ORDER BY c.line_number
```
> `i.value = 'UNIQUE'` is the marker, and it is EMPTY on a non-unique index — so treat
> the empty case as "not unique", not as "unknown". The covered columns come back in
> declaration order, which is semantic: a composite index serves a query that leads with
> its first column and not one that leads with its second.

#### 8. Refactoring Impact Analysis

Query templates to run with `graphit_ast_query`:
```bash
# COMPLETE impact of renaming/changing a function — all inbound edges
MATCH (dependent)-[r]->(target:Function {name: 'calculateTotal'}) RETURN dependent.name, label(dependent) AS dep_type, type(r) AS relation, dependent.path

# COMPLETE impact of changing an interface — implementors + callers of implementors
MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) RETURN impl.name, impl.path UNION ALL MATCH (impl)-[:IMPLEMENTS]->(iface:Interface {name: 'PaymentGateway'}) MATCH (caller)-[:CALLS]->(m:Function) WHERE m.path = impl.path RETURN caller.name AS name, caller.path AS path

# Safe-to-delete check — is this function called anywhere?
MATCH (caller)-[:CALLS]->(t:Function {name: 'legacyHelper'}) RETURN count(caller) AS caller_count

# Move-file impact — step 1: what the file declares (CALLS and CONTAINS cannot be joined, so this is two queries)
MATCH (f:File {path: 'src/utils/helpers.go'})-[:CONTAINS]->(e) WHERE label(e) IN ['Function', 'Method', 'Struct', 'Class', 'Interface', 'Type'] RETURN e.name, label(e) AS type

# Move-file impact — step 2: who outside the file calls those names (paste them into the IN list)
MATCH (outside)-[:CALLS]->(t:Function) WHERE t.name IN ['Helper', 'Format'] AND outside.path <> 'src/utils/helpers.go' RETURN t.name, count(outside) AS external_dependents ORDER BY external_dependents DESC
```

#### 9. Comments — searchable, without grep

Query templates to run with `graphit_ast_query`:
```bash
# Find a marker anywhere in the codebase's comments
MATCH (c:Comment) WHERE toLower(c.name) CONTAINS toLower('TODO') RETURN c.name, c.path, c.line_number ORDER BY c.path

# Every comment in one file, in reading order — the commentary of a file as a document
MATCH (f:File)-[:CONTAINS]->(c:Comment) WHERE f.path ENDS WITH 'handler.go' RETURN c.line_number, c.name ORDER BY c.line_number

# File skeleton with its commentary interleaved: comments and declarations, by line
MATCH (f:File {path: 'internal/auth/handler.go'})-[:CONTAINS]->(e) RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number

# Commented-out code left behind (a comment that reads like a statement)
MATCH (c:Comment) WHERE c.name CONTAINS '(' AND c.name CONTAINS ')' AND c.name CONTAINS ';' RETURN c.path, c.line_number, c.name

# Which files carry a licence header
MATCH (f:File)-[:CONTAINS]->(c:Comment) WHERE c.line_number = 1 AND toLower(c.name) CONTAINS 'copyright' RETURN f.path

# Comments near a declaration you care about — same file, adjacent lines
MATCH (f:File)-[:CONTAINS]->(fn:Function {name: 'ValidateToken'}) MATCH (f)-[:CONTAINS]->(c:Comment) WHERE c.end_line >= fn.line_number - 3 AND c.end_line < fn.line_number RETURN c.name, c.line_number
```

The last one exists because the file-and-line arithmetic is currently how you connect a
comment to what it documents: pair them through their shared `File` and their line numbers.

#### 10. Cross-Cutting Queries

Query templates to run with `graphit_ast_query`:
```bash
# Find all entry points (handlers, main functions, test functions) — no precomputed
# score for this; name the framework's own conventions yourself
MATCH (f:Function) WHERE f.is_exported AND (f.name = 'main' OR toLower(f.name) STARTS WITH 'test' OR toLower(f.name) CONTAINS 'handler') RETURN f.name, f.path

# Find all functions with high complexity (candidates for refactoring)
MATCH (f:Function) WHERE f.cyclomatic_complexity > 15 RETURN f.name, f.cyclomatic_complexity, f.path ORDER BY f.cyclomatic_complexity DESC

# Find orphan functions (never called — dead code candidates; check survivors against
# the framework's entry-point conventions before trusting the list)
MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called RETURN f.name, f.path

# Cross-language dependencies (e.g., Go calling a function defined in SQL)
MATCH (caller)-[:CALLS]->(callee) WHERE caller.lang <> callee.lang RETURN caller.name, caller.lang, callee.name, callee.lang, caller.path

# Which modules the project leans on hardest (IMPORTS only — mixing edge types matches nothing)
MATCH (f:File)-[:IMPORTS]->(m:Module) RETURN m.name, count(f) AS imported_by ORDER BY imported_by DESC LIMIT 30

# Annotation/decorator usage — find all entities with a specific annotation
MATCH (a:Annotation {name: 'Deprecated'})<-[:CONTAINS]-(owner) RETURN label(owner) AS type, owner.name, owner.path

# Parameter analysis — what parameters does a function expect?
MATCH (f:Function {name: 'createUser'})-[:HAS_PARAMETER]->(p:Parameter) RETURN p.name, p.value, p.line_number
```

### Phase 4: Source Code Extraction

The AST graph stores the **complete source code** of every indexed file. The `graphit_ast_source` tool
provides IDE-like capabilities to navigate source code efficiently — equivalent to `grep`, `head`, `tail`, and more.

> **This tool is the default way to read code here, including when you already know the path.**
> One call gives you a line range, a named entity, or a pattern with context — a plain file
> read gives you the whole file and charges you tokens for all of it. It also reaches imported
> contexts, whose files are not in this checkout.
> Read from disk with your native tools when the file is **not in the graph**: brand new,
> excluded by `.astignore`, or `ast.index_source` is `false`. This tool tells you when that is
> the case.

**Imported ast contexts** also may contain **source code** included in their imported graph,
try querying for it to understand the imported code and the overall behavior of the external contexts.

#### 4a. Get the entire source of a file

Call `graphit_ast_source` (passing absolute `project_dir` and `path`):
```
graphit_ast_source(project_dir: "/path/to/project", path: "internal/auth/handler.go")
```

#### 4b. Get source with line numbers — opt-in, off by default

`line_numbers` defaults to **false**, and leaving it there is the normal call: the numbers
are for a human reading the output, and asking for them widens every line you receive.
Pass it when the line numbers are what you actually need — reporting a location, or lining
an entity up against a range you got from the graph.

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

#### 4h. Source is NOT a graph property — do not query for it

File text lives in the search index, not on the `File` node, so there is no
`file.source` to return and a query asking for one gets nothing. The graph gives you
the location; `graphit_ast_source` gives you the text at it:
```
MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN file.path, fn.line_number, fn.end_line
```
then pass that `path` — with `entity` or the line range — to `graphit_ast_source`.

## 🧠 Complex Code Investigation — Agent-Driven Workflow

When you need to answer a complex, open-ended code question (e.g., "how does authentication work?",
"what's the impact of changing this interface?", "find all error handling patterns"), use this
multi-step agentic workflow. **You ARE the AI — do the analysis yourself, step by step.**
Combine **all your knowledge sources** — code graph, project memory, documentation wiki, and hub.

### Step 0: Check memory for prior knowledge
Call `graphit_memory_search` with keywords related to your question.
Someone (you or another agent) may have already investigated this — check for past decisions, skills, and facts.

### Step 1: Consult the knowledge wiki
Call `graphit_knowledge_search` to find architecture docs, specs, and ADRs about the area you're investigating.
The wiki contains pre-compiled project documentation — read it BEFORE diving into code.

### Step 2: Understand the AST graph schema
Call `graphit_ast_schema` to discover which node labels and relationships exist in this project's graph.

### Step 3: Discover relevant entities via hybrid search
Call `graphit_ast_search` with natural language keywords related to your question.
This returns the most relevant functions, classes, and modules ranked by BM25 + semantic similarity.

### Step 4: Write precise Cypher queries
Using the entity names and labels discovered in Step 3, write targeted Cypher queries via `graphit_ast_query`:
- Trace call chains: who calls these entities? What do they call?
- Map relationships: inheritance, imports, containment, field access
- Assess impact: find all inbound edges to understand coupling

### Step 5: Read source code for context
Call `graphit_ast_source` to extract the actual implementation of key entities discovered in Steps 3-4.
Use entity extraction (`entity` parameter) to get specific functions/methods without reading entire files.

### Step 6: Expand with external knowledge
If the code interacts with external systems, frameworks, or APIs — or the question itself is
about one, e.g. "how does <system> do X" rather than about this repository's code:
- Call `graphit_ast_list` first — the system may already be an imported context; querying an existing one beats re-searching the Hub
- Otherwise call `graphit_hub_search` with the system's name and `type: "knowledge"` (and `type: "ast"` if you need its source, not just its docs) to find pre-built artifacts
- Install relevant artifacts and consult their wikis/graph before guessing at API or system behavior
- Check `graphit_wiki_xrefs` for cross-references that connect code to documentation

### Step 7: Iterate and synthesize
If your initial queries don't fully answer the question:
- Refine search terms and re-run `graphit_ast_search`
- Follow new leads from call chains — query deeper into the graph
- Cross-reference AST findings with knowledge wiki and memory for full context
- **Synthesize** the answer yourself from all gathered sources — you ARE the AI

### Example: "How does the sync pipeline work?"
```
# Step 0: Check memory
graphit_memory_search(project_dir: "/path/to/project", query: "sync pipeline")

# Step 1: Check knowledge wiki
graphit_knowledge_search(project_dir: "/path/to/project", query: "sync pipeline")

# Step 2: Schema
graphit_ast_schema(project_dir: "/path/to/project")

# Step 3: Discover
graphit_ast_search(project_dir: "/path/to/project", query: "sync pipeline")

# Step 4: Trace (using names found in Step 3)
graphit_ast_query(project_dir: "/path/to/project", query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND toLower(f.name) CONTAINS 'sync' RETURN f.name, f.path, f.line_number, label(f) AS type")

# Step 4b: Call chain
graphit_ast_query(project_dir: "/path/to/project", query: "MATCH (a:Function {name: 'RunSync'})-[:CALLS]->(b) RETURN b.name, label(b) AS type, b.path")

# Step 5: Read implementation
graphit_ast_source(project_dir: "/path/to/project", path: "internal/sync/pipeline.go", entity: "RunSync")

# Step 6: Check hub for external dependencies
graphit_hub_search(query: "<dependency name>", type: "knowledge")
```

## Cypher Guidelines

- 🔒 **Schema before Cypher.** Call `graphit_ast_schema` before the first query of a session, and again after switching `project_dir` or `context`. Guessing a property name does not return an empty result — it crashes with `Binder exception: Cannot find property`.
- **IMPORTANT**: The `path` property is ALWAYS a relative path from the project root (e.g., `src/main.go`). Never use absolute paths when filtering `n.path`.
- **Node type is a LABEL, not a property.** Use `label(n)` — see Phase 1 for details.
- **Property names are exact.** Refer to the Property Reference table in Phase 1 — and to the list of properties that do **not** exist (`type`, `line`, `complexity`, `body`, `is_test`, …) with their real replacements. If a name is in neither the schema output nor the table, it does not exist.
- **A property binds when ANY candidate label has it — not when all of them do.** On an unlabeled `MATCH (n)`, `n.line_number` and `n.is_exported` are fine on a complete graph, and the column is simply empty for the nodes that lack them. What crashes is a property no reachable label has: `MATCH (n:Function) RETURN n.relative_path`, because `relative_path` belongs to `File`. So pin the label that actually carries what you are returning.
- **`WHERE label(n) IN [...]` does not fix a binding error.** Binding happens before filtering, so narrowing in the WHERE clause comes too late — the label has to be in the pattern (`MATCH (n:Function)`), and several labels means several MATCHes joined with `UNION ALL`.
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

## Code That Is Not In This Repository

**Never read another project's source file by file, and never grep its tree.** There are four
situations and each has a tool; text search is not one of them. Find your row first, because
choosing the wrong one wastes an import you did not need — or misses a graph you already have:

> This table is not only for planned refactors. It is exactly what to run when the user simply
> asks how a named system works, or whether it has some functionality: that question is a code
> question about a codebase that is not this one, and it gets the same lookup — find the row,
> then the tool. "I don't know that system" is never a reason to answer from general knowledge
> before checking whether it is a registered sibling or a Hub artifact.

| The code lives in | How to query it |
|---|---|
| **this repository** | the default — do not pass `context` |
| **a sibling project in the ecosystem** | **its own graph: pass its `dir` as `project_dir`.** No import, no context. Get the path from `graphit_cluster_projects` |
| **a checkout on this machine that is not a registered project** | import it once with `graphit_ast_install`, then pass `context` |
| **a dependency you do not have checked out** | `graphit_hub_search` with `type: "ast"` → `graphit_hub_install`, then pass `context` |

### 🔒 Check the ecosystem before you import anything

**Row two is the one that gets missed, and it is the cheapest.** A project registered in the
ecosystem already has its own indexed graph, its own compiled wiki, and its own memories — so
the moment you know its path you have every tool over there that you have here. Importing it
as a context re-indexes a graph that already exists.

So when the request names another project, service, or repository, **resolve it first**:

```
graphit_cluster_projects(project_dir: "/path/to/project")
```

Match the user's word against `name` and `description` — it rarely matches the directory name
— then use that entry's `dir`:

```
graphit_ast_search(project_dir: "<sibling dir>", query: "token validation")
graphit_ast_query(project_dir: "<sibling dir>", query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number, label(f) AS type")
graphit_ast_source(project_dir: "<sibling dir>", path: "<path from the query>", entity: "<name from the query>")
```

And pair it with that project's documentation and memories — `graphit_knowledge_search`,
`graphit_wiki_search` and `graphit_memory_search`, each with the
same `project_dir`. The graph says what the code does; the wiki and the memories over there say
**why**, which reading its source will not recover. The hub skill covers the full protocol.

> A wrong `project_dir` does not raise an error. It answers confidently about a different
> codebase, or returns nothing and reads exactly like "that code does not exist". Resolve the
> path with a tool; do not type one from memory.

### Imported contexts — for rows three and four

Every query tool takes an optional `context`, and the rest of this skill tells you not to pass
it for your own project. A context is a repository indexed **into your graph** and queried by
name, which is what you need when the code has no project of its own on this machine.

#### What is already imported
```
graphit_ast_list(project_dir: "/path/to/project")
```

Returns each context name and the repository path it was built from. **Call this before
assuming a `context` name exists** — a query with an unknown context fails, and a query
against the wrong one silently answers about the wrong codebase. It also tells you the import
already happened, so you do not redo it.

#### Importing a local repository as a context
```
graphit_ast_install(project_dir: "/path/to/project", path: "/absolute/path/to/other-repo", context: "other-repo")
```

`path` must be absolute and must already exist on this machine — this indexes a checkout,
it does not download anything. For a dependency you do **not** have checked out, the Hub is
the route: `graphit_hub_search` with `type: "ast"`, then `graphit_hub_install`.

Do **not** import a project that is registered in the ecosystem — that is row two, and it
already has a graph. Check `graphit_cluster_projects` first.

Then query it by name:
```
graphit_ast_query(project_dir: "/path/to/project", context: "other-repo", query: "MATCH (f:Function {name: 'Handle'}) RETURN f.path, f.line_number")
```

#### Removing a context
```
graphit_ast_remove(project_dir: "/path/to/project", context: "other-repo")
```

> ⚠️ **Called without `context`, this tool wipes the current project's entire graph**
> (`MATCH (n) DETACH DELETE n`) and every query afterwards returns nothing until a full
> reindex finishes. Never call it without `context` unless the user asked for exactly that.

## Index Maintenance — the tools, and when they are actually needed

### `graphit_ast_index` — reindex now, or reindex differently

The daemon already reindexes on file change (see the section on staleness below), so this is
not part of the normal edit loop. Reach for it when you need something the watcher does not do:

| Situation | Call |
|---|---|
| One directory, right now, before a query you are about to run | `path: "internal/auth"` |
| A file's extension is parsed by the wrong grammar | `grammar: ".pks=antlr-plsql"` |
| The graph looks wrong and you want the file re-parsed even though its hash is unchanged | `reindex: true` |
| The graph is corrupt and you want to start clean (destructive — drops the database) | `reset: true` |

```
graphit_ast_index(project_dir: "/path/to/project", path: "internal/auth")
```

### `graphit_ast_embed` — make semantic search work

`mode: "semantic"` and the semantic half of `mode: "hybrid"` need vectors. If semantic
results come back empty, the vectors are missing — not the code:
```
graphit_ast_embed(project_dir: "/path/to/project")
```

Fire-and-forget; hybrid search keeps working on FTS alone in the meantime. Prefer this over
a full `graphit_sync`: it does exactly this one job, where sync also reindexes
the AST, the wiki, and memory.

**Not every label has a vector, and that is a per-language declaration.** Each grammar's
query YAML lists the labels worth finding by meaning in `embed_labels`; anything absent
from that list is keyword-only. So a `semantic` search that returns nothing for a kind of
entity is not necessarily missing vectors — that language may simply not embed that label,
and re-running embed will not change it. **`entity_fts` indexes every entity by name**, so
`mode: "fts"` and `mode: "hybrid"` reach all of them either way.

`Comment` IS embedded wherever a grammar declares comments, which is what makes "find me
the note explaining why this is done this way" a semantic question rather than a guess at
which words the author used.

### The graph did not open — read this before falling back to grep

```
ladybug open: failed to open database with status 1
```

The daemon holds a write lock while it reindexes, and a read landing in that window fails
with the message above. It names the database, so it reads like "there is no graph here" —
**it is a lock, not an absence. Retry.** The same query succeeds seconds later.

Falling back to grep here is the most expensive mistake available in this framework: you
abandon the graph exactly because it was busy building itself, and the answer you produce
from text search is the worse one. If retrying keeps failing, call `graphit_daemon_status` —
`running: false` means nothing has been indexing at all, and `recent_logs` says why a rebuild
failed. A genuinely missing index reports itself differently: *no AST database found at ...*.

### `graphit_ast_export` — hand the graph to something else

```
graphit_ast_export(project_dir: "/path/to/project", format: "obsidian", output: "/tmp/graph-md")
graphit_ast_export(project_dir: "/path/to/project", format: "bundle", output: "/tmp/graph-bundle")
```

`format` and `output` are both required; only `obsidian` (browsable markdown) and `bundle`
(archive for sharing or importing elsewhere) are accepted. This is an export for a human or
another tool — **it is never the way to read code for yourself.** For that, query the graph.

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

## Reindexing is automatic — do not call sync after every edit

The daemon watches the source tree and reindexes incrementally when a file changes. It is
told the exact paths, so an edit costs about a second of its time and none of yours. After
you edit, create, rename, or delete a source file, **there is nothing to call.**

Calling `graphit_sync` after every edit is the most common way this module
gets misused: it redoes work the watcher already did, and on a large repository it makes you
wait on a full rebuild — AST, wiki, memory and Hub — to get an incremental result you were
going to have anyway.

**Reindex by hand only when the watcher cannot have seen the change:**

| situation | why the watcher missed it | what to call |
|---|---|---|
| the daemon is not running | nothing is watching — confirm with `graphit_daemon_status` | `graphit_ast_index` |
| code arrived from outside this machine — a pull, a checkout, a rebase, a restore | the daemon was down, or it landed as one bulk event | `graphit_ast_index` |
| a query returns something you know is stale, a minute after the edit | the reindex failed; the tool surfaces the error | `graphit_ast_index` with `path` |
| the file is parsed by the wrong grammar | the watcher reindexes with the configured grammar, which is the one that is wrong | `graphit_ast_index` with `grammar` |
| semantic search returns nothing | vectors are computed separately from the index | `graphit_ast_embed` |

In every one of those rows the targeted tool beats `graphit_sync`: same fix,
a fraction of the work. `graphit_sync` is for when you genuinely want every
subsystem rebuilt at once — which is exactly once per session, at the end, below.

### 🔒 MANDATORY: one `graphit_sync` when you finish a change session

**Before you report a session that changed code as done, call `graphit_sync`
once.** Not per edit — once, at the end, after the last file is written.

This is not a contradiction of the section above, and the distinction is the point:

| when | what to call | why |
|---|---|---|
| after each edit, mid-session | **nothing** | the watcher already did it, incrementally, in about a second |
| once, when the change session is finished | `graphit_sync` | it makes every index's currency a fact you established rather than one you assumed |

**Why sync and not `graphit_ast_index`, when this is the AST skill:** because a
session that changed code changed more than code. You wrote a task log, and most likely a
memory or two, and each of those lives in an index of its own. `graphit_ast_index`
would leave the graph current and the two wikis exactly as stale as they were — which is the
half-finished version of this check, and indistinguishable from the finished one afterwards.
Sync covers the code graph, the documentation wiki, the memory wikis and the Hub in one pass.

The reason it is worth one call: the watcher is the normal path and it is reliable, but it
is not a guarantee you checked. It can have been down for part of your session, a rename or
a bulk write can have arrived as one event it coalesced, and a failed reparse leaves an index
that answers confidently about something that no longer exists. The cost of finding out is
one incremental pass over paths that are almost all unchanged — hashes match, nothing is
reparsed. The cost of NOT finding out is paid by the next session, which opens a stale index
and has no way to tell.

### Mid-session, when you need CERTAINTY rather than eventual currency

The watcher is automatic but not instantaneous: it notices the write, waits out its
debounce, and rebuilds after it. Inside that window the graph answers from the previous
state — and it does so with exactly the confidence of a current answer, which is what makes
the lag dangerous rather than merely slow. You cannot tell a stale result from a fresh one
by looking at it.

So the trigger for calling `graphit_sync` mid-session is not "I edited" — it is
**"I am about to decide something on the basis of what this returns"**: a query whose result
you will act on right after writing the file it concerns, an impact check on code you just
moved, or any read that follows a change that did not come from your own edits (a pull, a
checkout, a rebase, a restore). In those cases call it and let it finish before you read.
When it is only the graph you doubt, `graphit_ast_index` with a `path` gets there
faster; `graphit_sync` is what puts the graph, the documentation wiki and the
memory wikis at the same point, which is what "the indexes are current" actually means.

So: **the last tool call of a session that touched code is `graphit_sync`.**
Reach for `graphit_ast_index` instead when the code graph alone is the
problem and you want it fixed now — the rows in the table above, mid-session.

**What is still on you is writing the code and its documentation** — see the documentation
skill. That obligation was never about reindexing.

## Where the graph lives, and why you cannot open it

Every graph on this machine is stored **once**, in the global brand directory, keyed by
whose it is:

```
<global>/ast/project/<project-id>/graph.icebug/        this project's graph
<global>/ast/context/<name>/graph.icebug/             a locally imported graph
<global>/ast/hub/<context-id>/<version>/              a Hub graph, shared per version
```

Nothing is copied into the project, and there is no `.graphit/ast/project` any
more. Two consequences for you:

1. **A graph is never a file you read.** `graphit_ast_source` is how source
   text is retrieved — it reads the indexed copy inside the store, which is also why it works
   on an imported context and on another project, neither of whose files are in this checkout.
2. **What a project knows about is a per-project record, not a directory.** Hub contexts are
   claimed by the project's lockfile and locally imported ones by its context registry, which
   is why `graphit_ast_list` needs a `project_dir` and answers differently
   for two projects sharing the same store.

