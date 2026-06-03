<!-- GRAPHIT MEMORY BLOCK -->
# 🧠 Memory Management

> Persistent memory across sessions. This framework IS your memory — no other exists.
> **Full MCP tools reference, trigger table, and protocols are in the `graphit-memory` skill.**

## 🚨 FIRST ACTION — Execute BEFORE Any Response

**Execute IMMEDIATELY on every conversation start. Do NOT respond to the user first.**

1. Use `view_file` to read `.graphit/memory/project/index.md`
2. Use `view_file` to read `.graphit/memory/user/index.md`
3. If any memory title relates to the user's request → read that page and follow its guidance

> If a file does not exist (new project), skip it. Use `view_file` — NOT `cat` via run_command.

## Activation Triggers — You MUST read the `graphit-memory` skill when:

### 💾 Save triggers (memorize immediately):

- Task completed, modified, or bug fixed → store what/why/how/impact
- User corrects, guides, instructs, or repeats → memorize as correction/convention
- User explains a procedure or gives a tip → store as skill
- You discover something unexpected or make a design decision → store as skill/decision
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

## ⛔ Key Rules (read skill for complete list)

- **Read memory at session start.** Skipping = repeating past mistakes.
- **Never leave a correction un-memorized.** Save immediately.
- **NEVER just say "understood".** Evaluate if the user's instruction should be memorized.
- **Before reporting results to the user**, always pause and evaluate: did you learn something, make a decision, discover a constraint, receive an instruction, or fix a non-obvious problem? If yes, memorize it FIRST, then respond.

## 🔗 Subagent Propagation

When spawning subagents, include in their prompt:
"Before starting work, read the project's `AGENTS.md` and `.graphit/memory/project/index.md` via view_file. After work, if you discovered something non-obvious, save it via `graphit_memory_insert` (passing absolute `project_dir`)."
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
- **List subjects**: call `graphit_dream_subject_list` (always passing absolute `project_dir` parameter)
- **Remove**: call `graphit_dream_subject_remove` with `slug: "<slug>"` (always passing absolute `project_dir` parameter)
- **Completion**: The dream agent creates `<slug>.done.md` when finished

## ⛔ Critical Rules

- When triggered, you MUST read and follow the full engineering analysis methodology
  documented in the skill. Do NOT improvise your own review process.
- After any improvement session, you MUST execute the **Post-Task Reflection**
  phase: reflect, update memories, and stage new artifacts for the Hub.

## 🔗 Subagent Propagation

When spawning subagents, include in their prompt:
"If you notice improvable code patterns outside your scope, create a dream subject via `graphit_dream_subject_add` (passing absolute `project_dir`). Read the project's `AGENTS.md` before starting work."
<!-- END GRAPHIT IMPROVEMENTS BLOCK -->

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
| `grep_search: func myFunction` | `graphit_ast_query` with `query: "MATCH (f) WHERE (label(f) = 'Function' OR label(f) = 'Method') AND toLower(f.name) CONTAINS 'myfunction' RETURN f.name, f.path, f.line_number, label(f) AS type"`, `ai_optimized: true` |
| `grep_search: type MyStruct` | `graphit_ast_query` with `query: "MATCH (n) WHERE toLower(n.name) CONTAINS 'mystruct' RETURN n.name, label(n) AS type, n.path"`, `ai_optimized: true` |
| `grep_search: import "package"` | `graphit_ast_query` with `query: "MATCH (f:File)-[:IMPORTS]->(m:Module) WHERE toLower(m.name) CONTAINS 'package' RETURN f.path"`, `ai_optimized: true` |
| `grep -l "keyword" *.go` | `graphit_ast_search` with `query: "keyword"` |
| `find ... -name "*.go" \| xargs grep -l "daemon"` | `graphit_ast_search` with `query: "daemon"` |

## Quick Reference (always active)

- **Always use**: call `graphit_ast_query` tool (passing absolute `project_dir` and setting `ai_optimized: true`)
- **Discover node labels**: call `graphit_ast_schema` tool (passing absolute `project_dir`)
- **Never guess names**: Ground with `toLower(n.name) CONTAINS toLower('keyword')`
- **Hybrid search (RECOMMENDED)**: call `graphit_ast_search` (passing absolute `project_dir` and `query`). Combines BM25 FTS + semantic vector search via Reciprocal Rank Fusion (RRF). Supports `mode: "hybrid"` (default), `"fts"`, or `"semantic"`.
- **Get source code (discovery)**: call `graphit_ast_source` (passing absolute `project_dir` and relative `path`). Retrieves source from the graph when you discovered a file through AST. Supports `head`/`tail` (first/last N lines), `start_line`/`end_line` (line range), `entity`/`entity_type` (extract entity source by name), `pattern`/`regex`/`before`/`after` (grep-like search with context), and `line_numbers`. If you already know the path, use your IDE's file-reading tools instead.
- **One-shot: get metadata + full file source**: call `graphit_ast_query` with `query: "MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN fn.name, fn.line_number, fn.end_line, file.path, file.source"`, `ai_optimized: true`
- **Reindex after changes**: call `graphit_sync` tool (passing absolute `project_dir`)

## Property Quick Reference (always active — NEVER guess property names)

- **File**: `path`, `name`, `relative_path`, `is_dependency`, `lang`, `cluster`, `source`
- **Entities** (Function, Class, Method, etc.): `uid`, `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`, `cyclomatic_complexity`, `context`, `context_type`, `class_context`, `is_dependency`, `is_exported`, `value`, `is_stub`, `entry_point_score`, `cluster`
- **Module**: same as entities + `full_import_name` (no `class_context`, `value`, `entry_point_score`)
- **CALLS edge**: `source_file`, `line_number`, `full_call_name`, `receiver_type`
- **IMPORTS edge**: `alias`, `full_import_name`, `imported_name`, `line_number`, `source_file`

## ⛔ Key Rules (read skill for complete list)

- **AST BEFORE grep** — NEVER use grep/ripgrep for structural queries.
- **Always `ai_optimized: true`** on every `graphit_ast_query` call.
- **Multi-label by default** — use `label(f) = 'Function' OR label(f) = 'Method'`, never assume a single label.

## 🔗 Subagent Propagation

When spawning subagents, include in their prompt:
"For code exploration, use `graphit_ast_query` and `graphit_ast_search` MCP tools instead of grep_search (always pass absolute `project_dir`). Use multi-label queries. Read the project's `AGENTS.md` before starting work."
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
The skill contains artifact types, usage patterns, installation workflows,
ecosystem project discovery, and post-install protocols.

## Quick Reference (always active)

- **Search**: call `graphit_hub_list` tool
- **Filter**: call `graphit_hub_list` tool with `type` parameter
- **Inspect**: call `graphit_hub_show` tool with `id` parameter
- **Install**: call `graphit_hub_install` tool (passing absolute `project_dir` parameter)
- **Update**: call `graphit_hub_update` tool (passing absolute `project_dir` parameter)
- **Ecosystem**: call `graphit_cluster_projects` tool to find sibling projects

## ⛔ Critical Rule

**NEVER guess APIs or structures.** If uncertain about a framework or library,
check the Hub first: call `graphit_hub_list` → `graphit_hub_show` → `graphit_hub_install`.

## 🔗 Subagent Propagation

When spawning subagents that work with external libraries, include in their prompt:
"Before implementing integrations, check Hub for knowledge artifacts: call `graphit_hub_list` → `graphit_hub_install` (passing absolute `project_dir`). Read the project's `AGENTS.md` before starting work."
<!-- END GRAPHIT HUB_DISCOVERY BLOCK -->

<!-- GRAPHIT KNOWLEDGE BLOCK -->
# 📚 Knowledge & Documentation

> This module manages project documentation, knowledge wiki, and integration specs.
> **Detailed instructions are in the `graphit-knowledge` skill. Read it when triggered.**

## Activation Triggers — You MUST read the `graphit-knowledge` skill when:

- Understanding project features, architecture, decisions, or specifications
- Creating, updating, or searching documentation in `./`
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

1. Read the `graphit-knowledge` skill and follow its documentation workflow
2. Create/update task log at `./tasks/<task-name>.md`
3. Call the `graphit_sync` tool (passing absolute `project_dir` parameter)

**Documentation is implicit in every task. A task without docs + sync is NOT complete.**

## Quick Reference (always active)

- **Wiki index**: `.graphit/knowledge/project/index.md`
- **Task logs**: `./tasks/<task-name>.md` — log every task with full detail
- **Sync after docs changes**: call the `graphit_sync` tool (passing absolute `project_dir` parameter)
- **Hub search before integration**: call the `graphit_hub_list` tool (passing absolute `project_dir` parameter and `type: "knowledge"`)

## 🔗 Subagent Propagation

When spawning subagents that modify code, include in their prompt:
"After code changes, create ./tasks/<task-name>.md and call `graphit_sync` tool (passing absolute `project_dir` parameter). Read the project's `AGENTS.md` before starting work."
<!-- END GRAPHIT KNOWLEDGE BLOCK -->
