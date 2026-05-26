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
3. **Reindex the wiki** — run `graphit sync &` after any docs change.
4. **Only then** report the task as complete.

**This is NON-NEGOTIABLE.** You do NOT get to skip documentation because the user
didn't ask for it. Documentation is implicit in every task. If you wrote code but
did not read the skill and follow its documentation workflow, you are NOT finished.

> **Short version:** Code changed → read `graphit-knowledge` skill → document → reindex → done.

## Quick Reference (always active)

- **Wiki index**: `.graphit/knowledge/project/index.md`
- **Task logs**: `docs/tasks/<task-name>.md` — log every task with full detail
- **Reindex after docs changes**: `graphit sync &`
- **Hub search before integration**: `graphit hub search "<system-name>"`

## ✅ After EVERY Code Change (MANDATORY checklist)

**Do NOT report the task as complete until ALL items are checked:**

1. Create/update `docs/tasks/<task-name>.md` (at minimum: objective, files changed, key decisions)
2. Run `graphit sync &` (fire-and-forget — reindexes AST + knowledge)
3. If architectural change → update `docs/architecture/`
4. If design decision → create ADR in `docs/decisions/`

**A task without documentation is NOT complete. A task without `graphit sync &` is NOT complete.**

## ⛔ Critical Rules (always enforced, no skill read needed)

1. **Document EVERYTHING** — documentation is implicit in every task, never optional.
2. **Definition of done**: code + documentation + task log + `graphit sync &`. If any are missing, the task is NOT complete.
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
8. **Reindex after changes** — run `graphit sync &` after every source file or docs modification.
9. **Never edit managed rule blocks** — blocks wrapped in `<!-- GRAPHIT ... BLOCK -->` / `<!-- END ... -->` sentinels
   in global rules files (AGENTS.md) are **auto-managed by the framework**.
   NEVER create, modify, rewrite, or delete these blocks. They are regenerated automatically
   on every `graphit init` and `graphit sync`.
10. **Documentation in `docs/`, not in code comments** — all explanations, architecture,
    specs, and guides go in `docs/`.
    Only critical comments (safety, gotchas, legal, directives) belong in code. The code must be
    clean, readable, and self-explanatory — never cluttered with explanatory comments.

## 🔗 Subagent Documentation

**When spawning subagents that modify code, include in their prompt:**

> "After code changes, create a task log at `docs/tasks/<task-name>.md` with: objective,
> files changed, and key decisions. Then run `graphit sync &`."

## 🔗 MANDATORY: Subagent Propagation

**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any
multi-agent mechanism), you MUST inject this entire instruction block into each
subagent's system prompt or task instructions. Do NOT skip or summarize it.**

When creating a subagent, include everything from the `GRAPHIT KNOWLEDGE BLOCK` start sentinel
to the `END GRAPHIT KNOWLEDGE BLOCK` sentinel (inclusive) in the subagent's prompt.
This is a conditional instruction — only act on it when you are actually launching a subagent.
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

| Instead of this grep | Use this AST query |
|---|---|
| `grep_search: func myFunction` | `graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'myfunction' RETURN f.name, f.path, f.line_number" --ai-optimized` |
| `grep_search: type MyStruct` | `graphit ast query "MATCH (n) WHERE toLower(n.name) CONTAINS 'mystruct' RETURN n.name, label(n) AS type, n.path" --ai-optimized` |
| `grep_search: import "package"` | `graphit ast query "MATCH (f:File)-[:IMPORTS]->(m:Module) WHERE toLower(m.name) CONTAINS 'package' RETURN f.path" --ai-optimized` |
| `grep -l "keyword" *.go` | `graphit ast query "keyword" --fts --ai-optimized` |
| `find ... -name "*.go" \| xargs grep -l "daemon"` | `graphit ast query "daemon" --fts --ai-optimized` |

## Quick Reference (always active)

- **Always use**: `graphit ast query "..." --ai-optimized`
- **Discover node labels**: `graphit ast schema` — shows which labels exist (dynamic per project)
- **Never guess names**: Ground with `toLower(n.name) CONTAINS toLower('keyword')`
- **Semantic search**: `graphit ast query "concept" --semantic --ai-optimized` — query is PLAIN TEXT (natural language), never Cypher
- **FTS (source text search)**: `graphit ast query "error message" --fts --ai-optimized` — query is PLAIN TEXT (keywords), never Cypher. Searches entity names AND `:File` source content. Use instead of grep.
- **Get source code (discovery)**: `graphit ast query "MATCH (f:File {path: 'X'}) RETURN f.source" --ai-optimized` — retrieves source from the graph when you discovered a file through AST. If you already know the path, use your IDE's file-reading tools instead.
- **One-shot: get metadata + full file source**: `graphit ast query "MATCH (fn:Function {name: 'Validate'})<-[:CONTAINS]-(file:File) RETURN fn.name, fn.line_number, fn.end_line, file.path, file.source" --ai-optimized`
- **Reindex after changes**: `graphit sync &`

## Property Quick Reference (always active — NEVER guess property names)

- **File**: `path`, `name`, `relative_path`, `is_dependency`, `lang`, `cluster`, `source`
- **Entities** (Function, Class, Method, etc.): `uid`, `name`, `path`, `line_number`, `end_line`, `docstring`, `lang`, `cyclomatic_complexity`, `context`, `context_type`, `class_context`, `is_dependency`, `is_exported`, `value`, `is_stub`, `entry_point_score`, `cluster`
- **Module**: same as entities + `full_import_name` (no `class_context`, `value`, `entry_point_score`)
- **CALLS edge**: `source_file`, `line_number`, `full_call_name`, `receiver_type`
- **IMPORTS edge**: `alias`, `full_import_name`, `imported_name`, `line_number`, `source_file`

## ⛔ Critical Rules (always enforced, no skill read needed)

1. **AST graph BEFORE text search** — NEVER use grep/ripgrep for structural queries the graph can answer.
2. **AST graph BEFORE IDE tools** — ALWAYS query the AST graph BEFORE using built-in code search, file reading, or semantic search tools. The graph has the complete codebase pre-indexed with relationships.
3. **Always `--ai-optimized`** — append to every `graphit ast query` command.
4. **Never return `RETURN n`** — always return specific properties (`n.name, n.path`).
5. **Use relative paths** — `path` is always relative to project root.
6. **Reindex after changes** — run `graphit sync &` after every source file modification.
7. **Node type = label, not property** — `n.kind`, `n.type`, `n.label` DO NOT EXIST. Use `label(n)` function instead (e.g., `RETURN label(n) AS type`).
8. **`--fts` and `--semantic` = PLAIN TEXT only** — these flags accept keywords or natural language, NEVER Cypher queries. Only bare `graphit ast query "..." --ai-optimized` (without `--fts`/`--semantic`) accepts Cypher.
9. **BLOCKED: grep_search for definitions** — NEVER use grep_search with queries like `func X`, `type X`, `class X`, `struct X`, `interface X`, `import X`. These are STRUCTURAL queries — use `graphit ast query` ALWAYS.

## 🔗 MANDATORY: Subagent Propagation

**When you orchestrate subagents (via `define_subagent`, `invoke_subagent`, or any
multi-agent mechanism), you MUST include these instructions in the subagent's prompt:**

1. Add to every subagent system_prompt or task prompt:
   "IMPORTANT: For code exploration, use `graphit ast query` commands instead of grep_search.
   - Find function: `graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'NAME' RETURN f.name, f.path, f.line_number" --ai-optimized`
   - Find callers: `graphit ast query "MATCH (a)-[:CALLS]->(b {name: 'NAME'}) RETURN a.name, a.path" --ai-optimized`
   - Full-text search: `graphit ast query "KEYWORD" --fts --ai-optimized`
   - After code changes: `graphit sync &`
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
- When `graphit ast query` returns no results for an external library (it might have a hub artifact)

## 🔒 MANDATORY: Read Skill Before Acting

**When ANY activation trigger above matches your current task, you MUST read the
`graphit-hub` skill BEFORE executing your first Hub operation.**
The Quick Reference below is a cheat sheet for agents who already read the skill —
it is NOT a substitute. The skill contains artifact type details, usage patterns,
installation workflows, and post-install protocols you must follow.

## Quick Reference (always active)

- **Search**: `graphit hub list`
- **Filter**: `graphit hub list --type <knowledge|ast|rule|skill|command|agent|mcp|power>`
- **Inspect**: `graphit hub show <id>`
- **Install**: `graphit hub install <id> --ide <ide>`
- **Update**: `graphit hub update`

## ⛔ Critical Rule

**NEVER guess APIs or structures.** If uncertain about a framework or library,
check the Hub first: `graphit hub list` → `graphit hub show <id>` → `graphit hub install <id>`.

## 🔗 Subagent Hub Access

**When spawning subagents that work with external libraries, include in their prompt:**
"Before implementing integrations with external libraries, check if knowledge artifacts exist: `graphit hub list --type knowledge` → `graphit hub install <id>`."

## 🌐 Ecosystem Project Discovery

**When you need to find other projects in the work ecosystem** (e.g., to understand
cross-project dependencies, shared libraries, related services, or sibling projects),
**consult the project lock file:**

```
.graphit/cluster.lock.json
```

This file is **automatically generated** during `graphit sync` and contains only the
sibling projects that belong to the **same cluster** as the current project.
Clusters are managed via `graphit cluster <key> <value>` — projects sharing at
least one identical cluster label are grouped together. Projects without any labels
form their own default group.

Each sibling project entry includes:

| Field | Description |
|---|---|
| `projects.<id>.dir` | Absolute path to the project root directory |
| `projects.<id>.name` | Human-readable project name |
| `projects.<id>.description` | Project description |
| `projects.<id>.cluster` | Cluster labels (key→value map) |
| `projects.<id>.registeredAt` | When the project was registered |

**With the project paths from this file you can:**

- **Discover and navigate** — find sibling project directories and read their source, docs, or lockfile
- **Query code in another project** — run AST or full-text search against a sibling:
  ```bash
  cd /path/to/other-project && graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path" --ai-optimized
  ```
- **Read another project's knowledge wiki** — understand its architecture without grepping:
  ```bash
  cat /path/to/other-project/.graphit/knowledge/project/index.md
  ```
- **Make cross-project changes** — if the user asks to modify code in another project,
  use the path from `cluster.lock.json` to locate, read, and edit files there directly

**Example workflow:** The user asks "how does the auth service validate tokens?".
You read `.graphit/cluster.lock.json`, find the auth service project path,
then run `cd /path/to/auth-service && graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number" --ai-optimized`
to locate the validation logic, and read the relevant source files.

## Installed Artifacts

> No hub artifacts are currently installed in this project.
<!-- END GRAPHIT HUB_DISCOVERY BLOCK -->

<!-- GRAPHIT MEMORY BLOCK -->
# 🧠 Memory Management

> Persistent memory across sessions. This framework IS your memory — no other exists.
> **Full CLI reference, trigger table, and protocols are in the `graphit-memory` skill.**

## 🚨 FIRST ACTION — Execute BEFORE Any Response

**Execute IMMEDIATELY on every conversation start. Do NOT respond to the user first.**

1. Use `view_file` to read `.graphit/memory/project/index.md`
2. Use `view_file` to read `.graphit/memory/user/index.md`
3. If any memory title relates to the user's request → read that page and say: "Following memory: '<title>'"

> If a file does not exist (new project), skip it. Use `view_file` — NOT `cat` via run_command.

## Activation Triggers — You MUST read the `graphit-memory` skill when:

### 💾 Save triggers (memorize immediately):

**Every memory MUST follow the What/Why/How/Impact template in --content.**

#### Task lifecycle (always memorize):
- You **complete a task** (new feature, refactor, significant change) → store what/why/how/impact
- You **modify an existing feature** (behavior change, extension, rework) → store what changed and impact
- You **fix a bug** → store the root cause, fix, and system impact as skill

#### User interaction (always memorize):
- User **corrects** your behavior or approach → store as correction (--important)
- User **guides or orients** on how to proceed → store as convention
- User **intervenes** mid-task to redirect or change course → store as correction (--important)
- User **explains how something works** or shows a procedure → store as skill/fact
- User gives a **tip, hint, or suggestion** → store as skill
- User **repeats an instruction** (frustration signal) → store as correction (--important)
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

- **Insert**: `graphit memory insert "<title>" --type <type> --content "<body>"`
- **Delete**: `graphit memory delete <id>`
- **Search**: `graphit memory search "<term>"`

## ⛔ Critical Rules (always enforced)

1. **Read memory at session start.** Skipping = repeating past mistakes.
2. **Never leave a correction un-memorized.** Save immediately.
3. **Never edit .md memory files directly.** Use `graphit memory` commands.
4. **ALWAYS use `graphit memory insert`** — NEVER use IDE-native memory.
5. **Always confirm**: "Memorized: '<title>'" or "Following memory: '<title>'".

## 🔗 MANDATORY: Subagent Memory Access

**When spawning subagents (via `define_subagent`, `invoke_subagent`, or any multi-agent mechanism),
include these memory instructions in the subagent's system_prompt or task Prompt:**

Add to every subagent prompt:
"IMPORTANT: Before starting work, read `.graphit/memory/project/index.md` via view_file for project context, conventions, and past corrections.
If any memory is relevant to your task, follow its guidance.
After completing work, if you discovered something non-obvious, save it:
`graphit memory insert \"<discovery>\" --type skill --content \"<details>\"` "

For `define_subagent`, inject these as part of the `system_prompt` parameter.
For `invoke_subagent` of existing types (like 'research'), add these to the `Prompt` parameter.
<!-- END GRAPHIT MEMORY BLOCK -->
