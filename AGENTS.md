<!-- GRAPHIT KNOWLEDGE BLOCK -->
# 📚 Knowledge & Documentation

> This module manages project documentation, knowledge wiki, and integration specs.
> **Detailed instructions are in the `graphit-knowledge` skill. Read it when triggered.**

## Activation Triggers — You MUST read the `graphit-knowledge` skill when:

- Understanding project features, architecture, decisions, or specifications
- Creating, updating, or searching documentation in `docs/`
- Working with external system integrations or API specifications
- Searching for project knowledge (wiki, backlinks, provenance)
- Discovering or documenting undocumented integrations

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-knowledge` skill BEFORE searching the wiki, creating documentation,
or working with integrations.** The Quick Reference below is a cheat sheet for agents
who already read the skill — it is NOT a substitute. The skill contains the full
wiki-first retrieval methodology, documentation templates, task log format, and
integration protocols you must follow.

## 🚨 MANDATORY POST-CHANGE PROTOCOL — After ANY Code Change

**After you modify, create, or delete ANY source file, you MUST:**

1. **Read the `graphit-knowledge` skill** — it contains the full documentation
   workflow, templates, and rules you must follow.
2. **Follow the skill's documentation workflow** — determine what type of change
   you made (architecture, feature, decision, bug fix, integration, etc.) and
   create or update the corresponding `docs/` files as specified in the skill.
3. **Sync the wiki** — call the `graphit_sync` tool (passing absolute `project_dir` parameter) after any docs change.
4. **Only then** report the task as complete.

**This is NON-NEGOTIABLE.** You do NOT get to skip documentation because the user
didn't ask for it. Documentation is implicit in every task. If you wrote code but
did not read the skill and follow its documentation workflow, you are NOT finished.

> **Short version:** Code changed → read `graphit-knowledge` skill → document → sync → done.

## Quick Reference (always active)

- **Wiki index**: `.graphit/knowledge/project/index.md`
- **Task logs**: `docs/tasks/<task-name>.md` — log every task with full detail
- **Sync after docs changes**: call the `graphit_sync` tool (passing absolute `project_dir` parameter)
- **Hub search before integration**: call the `graphit_hub_list` tool (passing absolute `project_dir` parameter and `type: "knowledge"`)

## ✅ After EVERY Code Change (MANDATORY checklist)

**Do NOT report the task as complete until ALL items are checked:**

1. Create/update `docs/tasks/<task-name>.md` (at minimum: objective, files changed, key decisions)
2. Call the `graphit_sync` tool (passing absolute `project_dir` parameter)
3. If architectural change → update `docs/architecture/`
4. If design decision → create ADR in `docs/decisions/`

**A task without documentation is NOT complete. A task without calling `graphit_sync` tool is NOT complete.**

## ⛔ Critical Rules (always enforced, no skill read needed)

1. **Document EVERYTHING** — documentation is implicit in every task, never optional.
2. **Definition of done**: code + documentation + task log + calling `graphit_sync` tool. If any are missing, the task is NOT complete.
3. **Log every task** — create/update `docs/tasks/<task-name>.md` with implementation details,
   technical debt, trade-offs, and system knowledge. Detail level must allow another agent to
   continue exactly where you left off.
4. **Document ALL use cases** — every task log MUST include a complete `## Use Cases` section
   listing every use case implemented or modified, with actor, preconditions, main flow,
   alternative flows, error scenarios, postconditions, and affected files. Use cases MUST be
   kept up-to-date on any future change — they are living documentation, never a one-time snapshot.
5. **Write ALL test cases & acceptance criteria** — every task log MUST include a
   `## Test Cases & Acceptance Criteria` section with BDD/Gherkin scenarios (Given/When/Then)
   covering every use case. Each scenario must be independent, traceable (Ref: UC-XX), and use
   specific test data. Cover happy paths, error cases, and boundary conditions. Test cases MUST
   be kept up-to-date on any future change.
6. **Never guess API contracts** — search the hub first, use knowledge wiki, never hallucinate.
7. **Wiki-first retrieval** — NEVER grep docs/ directly; always read `index.md` first.
8. **Reindex after changes** — call the `graphit_sync` tool after every source file or docs modification.
9. **Never edit managed rule blocks** — blocks wrapped in `<!-- GRAPHIT ... BLOCK -->` / `<!-- END ... -->` sentinels
   in global rules files (AGENTS.md) are **auto-managed by the framework**.
   NEVER create, modify, rewrite, or delete these blocks. They are regenerated automatically
   on every project initialization and sync.
10. **Documentation in `docs/`, not in code comments** — all explanations, architecture,
    specs, and guides go in `docs/`.
    Only critical comments (safety, gotchas, legal, directives) belong in code. The code must be
    clean, readable, and self-explanatory — never cluttered with explanatory comments.

## 🔗 Subagent Documentation

**When spawning subagents that modify code, include in their prompt:**

> "After code changes, create a task log at `docs/tasks/<task-name>.md` with: objective,
> files changed, and key decisions. Then call `graphit_sync` tool (passing absolute `project_dir` parameter)."

## 🔗 MANDATORY: Subagent Propagation

**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any
multi-agent mechanism), you MUST inject this entire instruction block into each
subagent's system prompt or task instructions. Do NOT skip or summarize it.**

When creating a subagent, include everything from the `GRAPHIT KNOWLEDGE BLOCK` start sentinel
to the `END GRAPHIT KNOWLEDGE BLOCK` sentinel (inclusive) in the subagent's prompt.
This is a conditional instruction — only act on it when you are launching a subagent.
<!-- END GRAPHIT KNOWLEDGE BLOCK -->

<!-- GRAPHIT AST_EXPLORER BLOCK -->
# 🔍 AST Code Exploration

> The AST graph database is your **PRIMARY and FIRST** code analysis tool.
> It is pre-indexed, faster, and more accurate than any text-based search.
> **Detailed instructions, query cookbook, and Cypher patterns are in the `graphit-ast` skill.**

## Activation Triggers — You MUST read the `graphit-ast` skill when:

- Finding where a function/class/method is defined
- Finding who calls a function or uses a class
- Understanding call hierarchy, inheritance chains, or import graphs
- Assessing impact of a code change (refactoring analysis)
- Finding unused code, high-complexity functions, or entry points
- Understanding file structure or module relationships
- Querying DML/database dependencies

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-ast` skill BEFORE executing your first AST query or code analysis action.**
The Quick Reference below is a cheat sheet for agents who already read the skill —
it is NOT a substitute. The skill contains the full phased exploration methodology,
Cypher guidelines, cookbook patterns, and fallback protocols you must follow.

## ⚡ grep → AST Translation (ALWAYS use AST instead of grep)

| Instead of this grep | Use this AST tool call (passing absolute `project_dir` parameter) |
|---|---|
| `grep_search: func myFunction` | `graphit_ast_query` with `query: "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'myfunction' RETURN f.name, f.path, f.line_number"`, `ai_optimized: true` |
| `grep_search: type MyStruct` | `graphit_ast_query` with `query: "MATCH (n) WHERE toLower(n.name) CONTAINS 'mystruct' RETURN n.name, label(n) AS type, n.path"`, `ai_optimized: true` |
| `grep_search: import "package"` | `graphit_ast_query` with `query: "MATCH (f:File)-[:IMPORTS]->(m:Module) WHERE toLower(m.name) CONTAINS 'package' RETURN f.path"`, `ai_optimized: true` |
| `grep -l "keyword" *.go` | `graphit_ast_search_fts` with `query: "keyword"` |
| `find ... -name "*.go" \| xargs grep -l "daemon"` | `graphit_ast_search_fts` with `query: "daemon"` |
| Searching for an entity that could be more than one type | `graphit_ast_query` with `query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND toLower(f.name) CONTAINS 'search' RETURN f.name, f.path, f.line_number, label(f) AS type"`, `ai_optimized: true` |

## Quick Reference (always active)

- **Always use**: call `graphit_ast_query` tool (passing absolute `project_dir` and setting `ai_optimized: true`)
- **Discover node labels**: call `graphit_ast_schema` tool (passing absolute `project_dir`)
- **Never guess names**: Ground with `toLower(n.name) CONTAINS toLower('keyword')`
- **Semantic search**: call `graphit_ast_search_semantic` (passing absolute `project_dir` and natural language `query`)
- **FTS (source text search)**: call `graphit_ast_search_fts` (passing absolute `project_dir` and keyword `query`). Searches entity names AND `:File` source content. Use instead of grep.
- **Get source code (discovery)**: call `graphit_ast_source` (passing absolute `project_dir` and relative `path`). Retrieves source from the graph when you discovered a file through AST. If you already know the path, use your IDE's file-reading tools instead.
- **One-shot: get metadata + full file source**: call `graphit_ast_query` with `query: "MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN fn.name, fn.line_number, fn.end_line, file.path, file.source"`, `ai_optimized: true`
- **Reindex after changes**: call `graphit_sync` tool (passing absolute `project_dir`)

## Property Quick Reference (always active — NEVER guess property names)

- **File**: `path`, `name`, `relative_path`, `is_dependency`, `lang`, `cluster`, `source`
- **Entities** (Function, Class, Method, etc.): `uid`, `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`, `cyclomatic_complexity`, `context`, `context_type`, `class_context`, `is_dependency`, `is_exported`, `value`, `is_stub`, `entry_point_score`, `cluster`
- **Module**: same as entities + `full_import_name` (no `class_context`, `value`, `entry_point_score`)
- **CALLS edge**: `source_file`, `line_number`, `full_call_name`, `receiver_type`
- **IMPORTS edge**: `alias`, `full_import_name`, `imported_name`, `line_number`, `source_file`

## ⛔ Critical Rules (always enforced, no skill read needed)

1. **AST graph BEFORE text search** — NEVER use grep/ripgrep for structural queries the graph can answer.
2. **AST graph BEFORE IDE tools** — ALWAYS query the AST graph BEFORE using built-in code search, file reading, or semantic search tools. The graph has the complete codebase pre-indexed with relationships.
3. **Always `ai_optimized: true`** — set parameter to `true` on every `graphit_ast_query` tool call.
4. **Never return `RETURN n`** — always return specific properties (`n.name, n.path`).
5. **Use relative paths** — `path` is always relative to project root.
6. **Reindex after changes** — call `graphit_sync` tool (passing absolute `project_dir`) after every source file modification.
7. **Node type = label, not property** — `n.kind`, `n.type`, `n.label` DO NOT EXIST. Use `label(n)` function instead (e.g., `RETURN label(n) AS type`).
8. **`graphit_ast_search_fts` and `graphit_ast_search_semantic` = PLAIN TEXT only** — these parameters accept keywords or natural language, NEVER Cypher queries. Only `graphit_ast_query` accepts Cypher.
9. **BLOCKED: grep_search for definitions** — NEVER use grep_search with queries like `func X`, `type X`, `class X`, `struct X`, `interface X`, `import X`. These are STRUCTURAL queries — call `graphit_ast_query` ALWAYS.

## 🔗 MANDATORY: Subagent Propagation

**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any
multi-agent mechanism), you MUST include these instructions in the subagent's prompt:**

1. Add to every subagent system_prompt or task prompt:
   "IMPORTANT: For code exploration, use `graphit_ast_query` and other AST MCP tools instead of grep_search (always pass absolute `project_dir` parameter):
   - Find function: call `graphit_ast_query` with `query: \"MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'NAME' RETURN f.name, f.path, f.line_number\"`, `ai_optimized: true`
   - Find callers: call `graphit_ast_query` with `query: \"MATCH (a)-[:CALLS]->(b {name: 'NAME'}) RETURN a.name, a.path\"`, `ai_optimized: true`
   - Full-text search: call `graphit_ast_search_fts` with `query: \"KEYWORD\"`
   - After code changes: call `graphit_sync` tool
   Read and follow all rules in the project's AGENTS.md before starting work."
2. For `define_subagent`, inject these as part of the `system_prompt` parameter.
3. For `invoke_subagent` of existing types (like 'research'), add these to the `Prompt` parameter.
<!-- END GRAPHIT AST_EXPLORER BLOCK -->

<!-- GRAPHIT HUB_DISCOVERY BLOCK -->
# 🔗 Hub Discovery

> Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, and powers.
> **Detailed instructions are in the `graphit-hub` skill.**

## Activation Triggers — You MUST read the `graphit-hub` skill when:

- Working with a third-party library, framework, or API you haven't used in this session
- Needing documentation or code examples for an external dependency
- Looking for reusable rules, skills, commands, agents, or MCP servers
- Setting up a new project or adding new dependencies
- When `graphit_ast_query` returns no results for an external library (it might have a hub artifact)

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-hub` skill BEFORE executing your first Hub operation.**
The Quick Reference below is a cheat sheet for agents who already read the skill —
it is NOT a substitute. The skill contains artifact type details, usage patterns,
installation workflows, and post-install protocols you must follow.

## Quick Reference (always active)

- **Search**: call `graphit_hub_list` tool
- **Filter**: call `graphit_hub_list` tool with `type` parameter
- **Inspect**: call `graphit_hub_show` tool with `id` parameter
- **Install**: call `graphit_hub_install` tool (passing absolute `project_dir` parameter)
- **Update**: call `graphit_hub_update` tool (passing absolute `project_dir` parameter)

## ⛔ Critical Rule

**NEVER guess APIs or structures.** If uncertain about a framework or library,
check the Hub first: call `graphit_hub_list` → `graphit_hub_show` → `graphit_hub_install`.

## 🔗 Subagent Hub Access

**When spawning subagents that work with external libraries, include in their prompt:**
"Before implementing integrations with external libraries, check if knowledge artifacts exist: call `graphit_hub_list` with `type: "knowledge"` → call `graphit_hub_install` with `id: "<id>"` (passing absolute `project_dir`)."

## 🌐 Ecosystem Project Discovery

**When you need to find other projects in the work ecosystem** (e.g., to understand
cross-project dependencies, shared libraries, related services, or sibling projects),
**call the `graphit_cluster_projects` tool (passing absolute `project_dir` parameter):**

```
graphit_cluster_projects(project_dir: "/path/to/project")
```

This tool returns a JSON map containing all sibling projects that belong to the **same cluster**
as the current project. Clusters are managed via `graphit_cluster_set`, `graphit_cluster_get`,
and `graphit_cluster_unset` MCP tools.

Each sibling project entry includes:

| Field | Description |
|---|---|
| `dir` | Absolute path to the project root directory |
| `name` | Human-readable project name |
| `description` | Project description |
| `cluster` | Cluster labels (key→value map) |
| `registeredAt` | When the project was registered |

**With the project paths from this tool you can:**

- **Discover and navigate** — find sibling project directories and read their source, docs, or lockfile
- **Query code in another project** — run AST query against a sibling (always pass its absolute path in the `project_dir` parameter):
  ```
  graphit_ast_query(project_dir: "/path/to/other-project", query: "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path", ai_optimized: true)
  ```
- **Read another project's knowledge wiki** — understand its architecture without grepping by using the `view_file` (or read file) tool on:
  ```
  /path/to/other-project/.graphit/knowledge/project/index.md
  ```
- **Make cross-project changes** — if the user asks to modify code in another project,
  use the path from the tool output to locate, read, and edit files there directly

**Example workflow:** The user asks "how does the auth service validate tokens?".
You call `graphit_cluster_projects` to find the auth service project path,
then call `graphit_ast_query` with `project_dir: "/path/to/auth-service"`, `query: "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number"`, and `ai_optimized: true` to locate the validation logic, and read the relevant source files.

## Installed Artifacts

> No hub artifacts are currently installed in this project.
<!-- END GRAPHIT HUB_DISCOVERY BLOCK -->

<!-- GRAPHIT MEMORY BLOCK -->
# 🧠 Memory Management

> Persistent memory across sessions. This framework IS your memory — no other exists.
> **Full MCP tools reference, trigger table, and protocols are in the `graphit-memory` skill.**

## 🚨 FIRST ACTION — Execute BEFORE Any Response

**Execute IMMEDIATELY on every conversation start. Do NOT respond to the user first.**

1. Use `view_file` to read `.graphit/memory/project/index.md`
2. Use `view_file` to read `.graphit/memory/user/index.md`
3. If any memory title relates to the user's request → read that page and say: "Following memory: '<title>'"

> If a file does not exist (new project), skip it. Use `view_file` — NOT `cat` via run_command.

## Activation Triggers — You MUST read the `graphit-memory` skill when:

### 💾 Save triggers (memorize immediately):

**Every memory MUST follow the What/Why/How/Impact template in the content parameter.**

#### Task lifecycle (always memorize):
- You **complete a task** (new feature, refactor, significant change) → store what/why/how/impact
- You **modify an existing feature** (behavior change, extension, rework) → store what changed and impact
- You **fix a bug** → store the root cause, fix, and system impact as skill

#### User interaction (always memorize):
- User **corrects** your behavior or approach → store as correction (with important: true)
- User **guides or orients** on how to proceed → store as convention
- User **intervenes** mid-task to redirect or change course → store as correction (with important: true)
- User **explains how something works** or shows a procedure → store as skill/fact
- User gives a **tip, hint, or suggestion** → store as skill
- User **repeats an instruction** (frustration signal) → store as correction (with important: true)
- User says "always/never/prefer/avoid/must" about code → store convention

#### Agent discoveries (always memorize):
- You make a design decision or choose between alternatives → record decision
- You **discover something unexpected** or solve a non-obvious problem → store as skill
- New instruction contradicts existing memory → replace it

### 📖 Read triggers (consult memory before acting):
- **Before implementing** any significant change → check for constraints and decisions
- **When stuck**, failing repeatedly, or facing a non-obvious problem → search for past solutions
- **Before proposing** architecture or a technical approach → check for prior decisions
- When trying to **understand project context** → search for institutional knowledge
- Memory management or maintenance tasks

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-memory` skill BEFORE executing your first memory operation.**
The Quick Reference below is a cheat sheet for agents who already read the skill —
it is NOT a substitute. The skill contains the full trigger→action table, memory types,
contradiction protocols, and transparency rules you must follow.
> **Exception:** The SESSION START PROTOCOL above is always active and does not require
> reading the skill — execute it immediately on every conversation.

## Quick Reference (always active)

- **Insert**: call `graphit_memory_insert` tool (passing absolute `project_dir` parameter)
- **Delete**: call `graphit_memory_delete` tool (passing absolute `project_dir` parameter)
- **Search**: call `graphit_memory_search` tool (passing absolute `project_dir` parameter)

## ⛔ Critical Rules (always enforced)

1. **Read memory at session start.** Skipping = repeating past mistakes.
2. **Never leave a correction un-memorized.** Save immediately.
3. **Never edit .md memory files directly.** Use `graphit_memory_*` MCP tools.
4. **ALWAYS use `graphit_memory_insert`** — NEVER use IDE-native memory.
5. **Always confirm**: "Memorized: '<title>'" or "Following memory: '<title>'".

## 🔗 MANDATORY: Subagent Memory Access

**When spawning subagents (via `define_subagent`, `invoke_subagent`, or any multi-agent mechanism),
include these memory instructions in the subagent's system_prompt or task Prompt:**

Add to every subagent prompt:
"IMPORTANT: Before starting work, read `.graphit/memory/project/index.md` via view_file for project context, conventions, and past corrections.
If any memory is relevant to your task, follow its guidance.
After completing work, if you discovered something non-obvious, save it by calling the `graphit_memory_insert` tool: `graphit_memory_insert` with `title: \"<discovery>\"`, `type: \"skill\"`, and `content: \"<details>\"` (always passing absolute `project_dir` parameter)."

For `define_subagent`, inject these as part of the `system_prompt` parameter.
For `invoke_subagent` of existing types (like 'research'), add these to the `Prompt` parameter.
<!-- END GRAPHIT MEMORY BLOCK -->

<!-- GRAPHIT IMPROVEMENTS BLOCK -->
# 🔧 Code Improvement Methodology

> Autonomous code improvement, audit, review, refactoring methodology, and dream subjects.
> Includes a **mandatory post-task reflection phase** for knowledge generation.
> **Full analysis methodology is in the `graphit-improvements` skill.**

## Activation Triggers — You MUST read the `graphit-improvements` skill when:

- User asks to **improve**, **audit**, **review**, or **refactor** the codebase
- User requests autonomous quality assessment
- User asks for code smell detection or best-practice enforcement
- After completing any significant task — for the **reflection & knowledge generation** phase
- When you notice code patterns that could be improved but are out of scope for the current task
- When you want to schedule work for later autonomous processing
- When you need to create, update, or codify recurring patterns as Hub artifacts (skills, commands, rules)

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-improvements` skill BEFORE starting any analysis, review, or improvement work.**
The skill contains the full engineering analysis methodology, phase-by-phase workflow,
and post-task reflection protocol. Do NOT improvise your own review process.

## 💤 Dream Subjects — Queue Work for Autonomous Dreams

When the user asks you to **schedule**, **queue**, or **leave something for the next dream**,
create a dream subject. The Dream module picks up pending subjects during idle periods.

- **Add via MCP**: call `graphit_dream_subject_add` with `title: "Title"`, `body: "Detailed instructions"` (always passing absolute `project_dir` parameter)
- **Add via file**: Create `.graphit/dream/subjects/<slug>.md` with a `# Title` heading and instructions
- **List subjects**: call `graphit_dream_subject_list` (always passing absolute `project_dir` parameter)
- **Remove**: call `graphit_dream_subject_remove` with `slug: "<slug>"` (always passing absolute `project_dir` parameter)
- **Completion**: The dream agent creates `<slug>.done.md` when finished

## 🔗 Subagent Improvements

**When spawning subagents, include in their prompt:**
"If you notice code patterns that could be improved but are outside your current scope, create a dream subject by calling the `graphit_dream_subject_add` tool: `graphit_dream_subject_add` with `title: "<improvement title>"`, `body: "<detailed description of what to improve and why>"` (always passing absolute `project_dir` parameter)."

## ⛔ Critical Rules

- When triggered, you MUST read and follow the full engineering analysis methodology
  documented in the skill. Do NOT improvise your own review process.
- After any improvement session, you MUST execute the **Post-Task Reflection**
  phase: reflect, update memories, and stage new artifacts for the Hub.

## 🔗 MANDATORY: Subagent Propagation

**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any
multi-agent mechanism), you MUST inject this entire instruction block into each
subagent's system prompt or task instructions. Do NOT skip or summarize it.**

When creating a subagent, include everything from the `GRAPHIT IMPROVEMENTS BLOCK` start sentinel
to the `END GRAPHIT IMPROVEMENTS BLOCK` sentinel (inclusive) in the subagent's prompt.
This is a conditional instruction — only act on it when you are actually launching a subagent.
<!-- END GRAPHIT IMPROVEMENTS BLOCK -->
