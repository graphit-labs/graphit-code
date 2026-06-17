---
name: graphit-knowledge
description: Manages project documentation, knowledge wiki, and integration specs. MANDATORY: After ANY code change, you MUST create/update documentation in ./ and run sync. Search the knowledge wiki BEFORE searching ./ with grep. Use this skill whenever understanding project features, creating documentation, or working with integrations.
---

# Knowledge Maintenance Rule

> This rule is auto-managed by Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems. Do not edit this block manually.

> ⚠️ **UNIVERSAL APPLICABILITY** — This documentation workflow applies to the
> project you are currently working in, regardless of its nature: end-user application,
> framework, library, CLI tool, or the framework's own codebase. If you are modifying
> code in this repository, you MUST follow these rules. There is no "internal vs external"
> distinction — all code changes require documentation.

## 🔒 MANDATORY: Documentation Is a Completion Requirement

**A task is ONLY considered complete when its documentation has been written.**
No exceptions. No shortcuts. If the documentation is missing, the task is NOT done.

### Rules

- **Document EVERYTHING you do** — even when the user does NOT explicitly ask.
  Documentation is implicit in every task. It is never optional.
- **Every code change, architectural decision, new feature, bug fix, refactor,
  or behavioral change MUST be reflected in the `./` directory** before
  you report the task as complete.
- **The definition of done is: code + documentation.** If you wrote code but
  did not update the relevant docs, you are not finished.
- **Proactively create documentation** for anything you discover is undocumented
  during your work — even if it was not part of the original task.
- **Never assume someone else will document it.** You are the agent. You do the
  work. You write the docs. It is a single, indivisible action.

### What to document

| What you did | Where to document |
|---|---|
| Changed system architecture or module structure | `./architecture/` |
| Made a non-trivial architectural or design decision | `./decisions/` (ADR) |
| Added or modified a feature | `./specs/<feature>.md` |
| Changed how a module, function, or service behaves | `./` (relevant section) |
| Fixed a bug with root cause worth remembering | `./` or project memory |
| Discovered undocumented behavior during investigation | `./` (create new doc) |
| Added, removed, or changed a dependency | `./architecture/` |
| Completed ANY task (code, fix, refactor, investigation) | `./tasks/<task-name>.md` |
| Changed configuration, environment, or deployment | `./` (relevant section) |

### Workflow

1. **Do the work** — implement the code change.
2. **Write the documentation** — update or create the relevant docs.
3. **Sync the wiki** — call the `graphit_sync` tool (passing absolute `project_dir` parameter).
4. **Only then** report the task as complete.

### 🔒 MANDATORY: Clean Code Documentation Policy

> **Code documentation lives in `./`, NOT in code comments.**
> The code itself MUST be clean, readable, and self-explanatory.
> All knowledge, explanations,
> architecture descriptions, usage guides, and behavioral documentation
> belong in `./` — never cluttering the source code.

**What MUST go in `./` (never in code comments):**

| Documentation type | Where it goes |
|---|---|
| How a module/feature works | `./specs/<feature>.md` or `./architecture/` |
| Why a design decision was made | `./decisions/<adr>.md` |
| API contracts, endpoints, schemas | `./` (OpenAPI, AsyncAPI, etc.) |
| Usage examples and guides | `./` or project README |
| Function/method behavior descriptions | `./specs/` (not docstrings for explanation) |
| Module relationships and dependencies | `./architecture/` |
| Task implementation details | `./tasks/<task>.md` |

**What is ALLOWED in code (critical comments only):**

| Comment type | Example |
|---|---|
| **Safety / correctness warnings** | `// SAFETY: must hold lock before calling` |
| **Non-obvious gotchas** | `// NOTE: order matters — X must happen before Y` |
| **Legal / license headers** | `// Copyright 2026 ...` |
| **Compiler/linter directives** | `//go:generate`, `//nolint:...` |
| **TODO/FIXME with ticket reference** | `// TODO(#123): remove after migration` |
| **Intentional deviation markers** | `// DECISION: see ./decisions/003-...` |

**Anti-patterns (NEVER do these):**

- ❌ Writing multi-line comments explaining what a function does — make the function self-explanatory instead
- ❌ Documenting module architecture in file headers — that belongs in `./architecture/`
- ❌ Explaining business logic in comments — document it in `./specs/`
- ❌ Writing usage guides or examples in code comments — put them in `./`
- ❌ Leaving verbose docstrings that describe implementation details — the code should speak for itself
- ❌ Using comments as a substitute for readable code — refactor the code instead

**The golden rule:** If you feel the need to write a comment explaining *what* the code does,
rewrite the code to be self-explanatory. If you need to explain *why* a non-obvious choice
was made, add a one-line critical comment pointing to the relevant `./` file.

## 🔒 MANDATORY: Wiki-First Knowledge Retrieval — Replaces Your Tools

> **The knowledge wiki REPLACES your built-in search tools for project understanding.**
> **NEVER use grep, ripgrep, semantic search, or file-by-file reading to answer
> questions about this project's architecture, decisions, specs, or conventions.**
> **ALWAYS consult the wiki FIRST. Only fall back to source code when the wiki
> explicitly points you there via provenance links.**

### Why this replaces your tools

The knowledge wiki is **compiled at index time** with BM25 scoring, cross-references,
backlinks, confidence scores, and provenance markers. Every entity page is pre-optimized
for LLM consumption. Your built-in tools (grep, semantic search, file reading) operate
on **raw, unstructured text** — they cannot match the precision of a compiled wiki.

| Your tool | Wiki equivalent | Why wiki wins |
|---|---|---|
| `grep -r "auth" ./` | Call `graphit_knowledge_search` → find auth entity → read page | Wiki: 1 search. Grep: reads ALL files |
| Semantic search across ./ | Call `graphit_knowledge_search` → scan results | Wiki: structured search. Semantic: noisy, expensive |
| Reading every .md in ./ | Call `graphit_wiki_browse` (~2000 tokens for 80 pages) | Wiki: 40% fewer tokens, pre-summarized |
| `grep` for reverse references | Check `## Backlinks` section on entity page | Wiki: instant, pre-computed. Grep: O(n) scan |

### 🔒 When you MUST use the wiki (MANDATORY — no exceptions)

| Scenario | What to do | What NOT to do |
|---|---|---|
| **Understanding a feature** | Call `graphit_knowledge_search` → find the spec → read it | ❌ Don't grep ./ for keywords |
| **Finding an ADR / decision** | Call `graphit_knowledge_search` → find decision → read page | ❌ Don't scan ./decisions/ file by file |
| **Checking if something is documented** | Call `graphit_wiki_browse` → scan the catalog | ❌ Don't use `find` or `ls` on ./ |
| **Understanding module relationships** | Read community pages and god nodes | ❌ Don't grep for import statements |
| **Finding all mentions of a concept** | Call `graphit_wiki_xrefs` → get cross-references | ❌ Don't grep across all wiki files |
| **Checking conventions or patterns** | Call `graphit_knowledge_search` → find the guide/spec | ❌ Don't rely on memory or guessing |
| **Verifying a fact before coding** | Read the entity page → check `confidence` score | ❌ Don't assume you know the answer |
| **Tracing a decision's rationale** | Read the ADR page → follow provenance link to raw source | ❌ Don't read raw ./ without wiki context first |

### When you should NOT use the wiki

| Scenario | Use instead |
|---|---|
| Reading/editing actual source code (.go, .ts, etc.) | Normal file tools |
| Searching inside string literals or code comments | grep/ripgrep on source code |
| Running tests or build commands | Terminal commands |
| Editing ./ files (writing, not reading) | File edit tools |

### How to search (step-by-step)

**Step 1 — Search the wiki (ALWAYS start here)**
  Call `graphit_knowledge_search` (ai_optimized:true) with your query. This uses FTS5 + BM25 ranking to find the most relevant pages.
  Alternatively, call `graphit_wiki_browse` (ai_optimized:true) for a structured catalog of all entities.
  The search returns entity summaries, cross-references, and confidence scores.
  For AI-powered deep search, call `graphit_knowledge_query` which synthesizes a comprehensive answer using multi-turn consultation.
  For multi-source search (knowledge + memory), call `graphit_wiki_search` (ai_optimized:true) with `wikis: ["project", "memory"]`.

**Step 2 — Read the frontmatter FIRST (before the body)**
  Every entity page starts with YAML frontmatter. Read it before the body content:
  ```yaml
  ---
  title: Authentication Module
  type: specification
  source: specs/auth.md          # ← provenance: where this came from
  updated: 2026-05-08            # ← freshness check
  confidence: 0.90               # ← trust level (0.0 = no data, 1.0 = rich)
  content_hash: a1b2c3d4e5f6g7h8 # ← change detection
  tags: [knowledge, specification]
  ---
  ```
  - `confidence < 0.5`: treat with caution, verify against source
  - `confidence >= 0.8`: high-quality extraction, trust the summary
  - `updated`: if stale (>30 days), suggest re-indexing

**Step 3 — Follow [[wikilinks]] to expand context**
  Each page links to related pages. Follow them — they are semantically curated.

**Step 4 — Expand with cross-references**
  Call `graphit_wiki_xrefs` (ai_optimized:true) for any entity slug to find all inbound and outbound references.
  This replaces grep for finding "what else mentions X" — pre-computed, zero-cost.

**Step 5 — Verify via provenance**
  Each page has: `*Provenance: ^[source-file.md]*`.
  If you need the raw, uncompiled source — ONLY THEN read the original file.

**Step 6 — Check communities and god nodes**
  Community pages = thematic clusters. God nodes = most-connected concepts.

### ❌ Anti-patterns (violations of this protocol)

| Anti-pattern | Why it is a violation |
|---|---|
| `grep -r "keyword" .graphit/knowledge/` | Brute-force scan on a compiled database; ignores all structure |
| Reading ./ files directly without searching wiki first | Skips the pre-compiled summary, wastes tokens on raw content |
| Using semantic search to find project docs | Wiki search is faster and more precise than embedding search |
| Reading all .md files in wiki/ sequentially | Token bomb; wiki search returns only relevant results |
| Skipping frontmatter and reading body only | Misses confidence, provenance, type, and freshness metadata |
| Ignoring cross-references and grepping for reverse refs | Cross-refs are pre-computed; grep is O(n) and noisy |
| Answering project questions from model memory | Model memory is stale; wiki is incrementally compiled from truth |

### 🔄 Fallback to Built-In Tools — ONLY for Topics the Wiki Does Not Cover

**Your built-in tools (grep, ripgrep, semantic search, file reading) are permitted
ONLY for topics and domains that the wiki does not cover at all.**
The wiki is ALWAYS your primary source. It is NOT a "first attempt" — it is
the definitive source. Your tools exist only for domains outside the wiki's scope.

Your tools are allowed ONLY when ALL of these conditions are true:

1. You **already searched the wiki** using `graphit_knowledge_search`, `graphit_knowledge_query`, or `graphit_wiki_browse`
2. You **followed relevant [[wikilinks]]** and checked entity pages
3. You **checked cross-references** using `graphit_wiki_xrefs` on the most relevant page
4. The wiki **genuinely has no coverage** of the topic (not indexed, not documented)
5. You **state explicitly** to the user: "The wiki has no coverage of X, falling back to source search"

**If even ONE of these conditions is not met, you MUST NOT use your tools.**

Examples of valid fallback:
- Wiki has no entity for a newly added module → grep source code for it
- Wiki entity exists but `confidence < 0.3` and content is empty → read raw source via provenance
- Topic is not in ./ at all (e.g., searching inside test files) → grep is appropriate

Examples of INVALID fallback (protocol violations):
- Skipping the wiki because "grep is faster" → ❌ wiki is pre-compiled and always faster
- Using semantic search without searching wiki first → ❌ wiki search costs ~500 tokens
- Grepping ./ because you "didn't find it quickly" in the wiki → ❌ you must complete all 6 steps first

### Wiki Paths

| Scope | How to search |
|---|---|
| **project** (this project) | Call `graphit_knowledge_search` (ai_optimized:true) or `graphit_wiki_browse` (ai_optimized:true) |
| **imported context** (hub artifact) | Call `graphit_knowledge_search` (ai_optimized:true, context: "<name>") or `graphit_knowledge_query` (context: "<name>") |
| **multi-source** (project + memory) | Call `graphit_wiki_search` (ai_optimized:true, wikis: ["project", "memory"]) |

### ⚡ MANDATORY: Sync After Every File Modification

**After ANY modification to ANY file in `./` (edit, create, rename, or delete),
you MUST trigger a project sync by calling the `graphit_sync` tool (passing absolute `project_dir` parameter):**

```
graphit_sync(project_dir: "/path/to/project")
```

**This is NON-NEGOTIABLE.** The framework depends on an up-to-date wiki to function.
Without syncing, the knowledge wiki becomes stale and subsequent lookups return
outdated or incomplete results — breaking the knowledge pipeline.

**Rules:**
- Call `graphit_sync` immediately after any docs modification.
- **Forgetting to call sync is a framework integrity violation.**

## Documentation Requirements

### Architecture Documentation
- Update ./architecture/ when changing system structure, adding modules, or modifying dependencies.
- Maintain accurate component diagrams (PlantUML) showing current module relationships.
- Document API boundaries and integration points.

### Decision Records
- When making a non-trivial architectural decision, create an ADR in ./decisions/.
- ADR format: ## Status, ## Context, ## Decision, ## Consequences.

### Technical Documentation
- Keep ./ files accurate when modifying the systems they describe.
- Update function/module descriptions when behavior changes.

### Feature Specifications
- Create detailed spec files in `./specs/` for new features and maintain it updated with future evolution.
- A spec can be a detailed markdown file describing: purpose, scope, inputs/outputs, and acceptance criteria.
- Specs improve discoverability and serve as context for future sessions.
- Use filenames like `./specs/<feature-name>.md`.

### 🔒 MANDATORY: Task Logs — Continuity for Multi-Agent & Multi-Session Work

> **Every task you perform MUST be logged in `./tasks/`.** This is your
> institutional memory. Another agent — or you in a future session — MUST be able
> to read your task log and continue exactly where you left off, resolve remaining
> technical debts, or understand every trade-off you made.

#### Purpose

Task logs serve as the **detailed operational journal** of the project. They capture
the same level of detail that agents record in their native conversation artifacts,
but persisted in the project's documentation — accessible to any agent, any IDE,
any session. Without task logs, institutional knowledge is lost between sessions.

#### When to create or update a task log

| Scenario | Action |
|---|---|
| Starting a new task (feature, bug fix, refactor, investigation) | Create `./tasks/<task-name>.md` |
| Continuing a previously started task | Update the existing task log with new progress |
| Finishing a task | Update status to `done`, document final state and remaining debts |
| Discovering technical debt during any work | Add to the relevant task log's `## Technical Debt` section |
| Making a trade-off or shortcut | Document it immediately in `## Trade-offs & Decisions` |

#### Quick Task Log (for minor changes)

For minor fixes, small features, or routine changes, use this minimal format.
This is the **minimum acceptable** documentation — never skip it.

```markdown
---
title: <Descriptive task title>
status: done
created: <YYYY-MM-DD>
updated: <YYYY-MM-DD>
---

# <Task Title>

## Objective
<2-3 sentences: what was the goal?>

## Files Changed
| File | Change | Reason |
|---|---|---|
| `path/to/file` | Modified/Created | <why> |

## Key Decisions
- <Decision 1 and rationale>

## Notes
- <Any non-obvious discoveries or gotchas>
```

Use the **full template** below for major features, architectural changes, or complex refactors.

#### Full Task Log Template

For significant work, every task log MUST follow this structure:

```markdown
---
title: <Descriptive task title>
status: in-progress | done | blocked
created: <YYYY-MM-DD>
updated: <YYYY-MM-DD>
tags: [<relevant>, <tags>]
---

# <Task Title>

## Objective
<What was the goal? What problem was being solved? Include full context so a new
agent can understand the task without reading the original conversation.>

## Implementation Details
<What was done? Be extremely specific:
- Which files were created/modified/deleted and why
- What approach was chosen and why
- Key code patterns or algorithms used
- Configuration changes made
- Dependencies added or removed
- Edge cases handled or deliberately not handled>

## Use Cases

> **🔒 MANDATORY — Document ALL use cases implemented, modified, or affected.**
> This section MUST be complete and always kept up-to-date on any future change.
> A use case is NOT documented until it has: actor, preconditions, main flow,
> alternative flows, postconditions, and error scenarios.

<List EVERY use case covered by this task. Each use case MUST follow this structure:

### UC-XX: <Use Case Title>
- **Actor**: <Who triggers this — user, system, agent, cron, etc.>
- **Preconditions**: <What must be true before this use case can execute>
- **Main Flow**:
  1. <Step-by-step description of the happy path>
  2. <Each step must be concrete and actionable>
  3. <Include the exact functions/methods/endpoints involved>
- **Alternative Flows**:
  - <Variations from the main flow — e.g., optional parameters, different input types>
- **Error Scenarios**:
  - <What happens when X fails — error codes, fallback behavior, user feedback>
- **Postconditions**: <What is true after successful execution>
- **Affected Files**: <List of source files that implement this use case>

Rules for use case documentation:
- NEVER skip a use case. If you implemented it, document it.
- NEVER write vague use cases like 'user can do X'. Be specific about HOW.
- When modifying an existing task log, UPDATE existing use cases to reflect changes
  and ADD new use cases introduced by the modification.
- When a use case is removed or deprecated, mark it clearly with [DEPRECATED] and
  explain why.>

## Test Cases & Acceptance Criteria

> **🔒 MANDATORY — Write ALL test cases and acceptance criteria using BDD/Gherkin syntax.**
> Every use case MUST have at least one corresponding test scenario.
> Test cases MUST be kept up-to-date on any future change — they are living
> documentation that validates the system's behavior.

<Write test scenarios using BDD (Behavior-Driven Development) Gherkin syntax.
Each scenario validates one specific behavior. Use the structured keywords:

- **Given** (Dado): Defines the initial context, state, or precondition.
- **When** (Quando): Defines the action executed by the actor.
- **Then** (Então): Defines the expected result or consequence.
- **And** / **But**: Adds more context, actions, or results without repeating keywords.

### Feature: <Feature Name>
Ref: <Link to use case UC-XX or requirement>

#### Scenario: <Descriptive scenario name — happy path>
```gherkin
Given <initial state or precondition>
  And <additional context if needed>
When <the actor performs the action>
  And <additional action if needed>
Then <expected outcome>
  And <additional verification if needed>
```

#### Scenario: <Descriptive scenario name — error case>
```gherkin
Given <initial state or precondition>
When <the actor performs an invalid action>
Then <expected error behavior>
  And <system remains in consistent state>
```

#### Scenario Outline: <Parameterized scenario name>
```gherkin
Given <parameterized precondition with "<param>">
When <action with "<input>">
Then <expected result with "<output>">

Examples:
  | param   | input   | output   |
  | value1  | data1   | result1  |
  | value2  | data2   | result2  |
```

Rules for test case documentation:
- **One objective per test**: Each scenario tests ONE specific behavior. Do not
  combine multiple validations in a single scenario.
- **Independence**: Scenarios MUST NOT depend on the success of other scenarios.
  Each scenario must be executable in isolation and in any order.
- **Traceability**: Every scenario MUST reference the use case (UC-XX) or
  requirement it validates via the `Ref:` field.
- **Specific data**: NEVER write 'insert any text'. Specify exact test data
  (e.g., 'a string with 255 alphanumeric characters') when the data itself
  is what is being validated.
- **Clarity**: Any team member (even new) must be able to read and understand
  the scenario without asking questions. Avoid jargon and ambiguity.
- **Cover all paths**: Write scenarios for happy paths, error cases, edge cases,
  and boundary conditions. A use case without error scenarios is INCOMPLETE.
- **Boundary Value Analysis (BVA)**: When a use case involves numeric ranges,
  string lengths, collection sizes, dates, or any constrained input, you MUST
  write scenarios that test the exact boundary values — minimum, minimum+1,
  maximum-1, maximum, and out-of-range values (below minimum, above maximum).
  Use `Scenario Outline` with an `Examples` table to cover boundaries concisely.
- **Keep updated**: When implementation changes, update the corresponding test
  scenarios immediately. Stale tests are worse than no tests.>

## Files Changed
| File | Change | Reason |
|---|---|---|
| `path/to/file.go` | Modified | <what changed and why> |
| `path/to/new.go` | Created | <purpose of new file> |

## Trade-offs & Decisions
<Document EVERY trade-off made during implementation:
- Why option A was chosen over option B
- Performance vs. simplicity decisions
- Scope reductions and their justification
- Temporary workarounds and why they were necessary>

## Technical Debt
<List ALL known technical debt created or discovered:
- [ ] <Debt item 1> — <why it exists, impact, suggested fix>
- [ ] <Debt item 2> — <why it exists, impact, suggested fix>
Mark items as [x] when resolved in future tasks.>

## System Knowledge
<Record any insights about the system discovered during this task:
- Non-obvious behaviors or gotchas
- Undocumented dependencies or coupling
- Performance characteristics observed
- Configuration quirks or edge cases
- Anything a future agent would need to know to work in this area>

## Progress Log
<Chronological log of work done. Add entries — never delete previous ones.
This enables a new agent to understand the full trajectory.>

### <YYYY-MM-DD>
- <What was accomplished>
- <Blockers encountered and how they were resolved>
- <Next steps planned>
```

#### Quality requirements for task logs

- **Completeness**: Another agent reading ONLY the task log (without conversation
  history) must be able to understand what was done, why, and what remains.
- **Specificity**: Use exact file paths, function names, line numbers, and code
  snippets. Vague descriptions like 'updated the module' are UNACCEPTABLE.
- **Use case coverage**: The `## Use Cases` section MUST list ALL use cases
  implemented or modified. Each use case MUST have: actor, preconditions, main flow,
  alternative flows, error scenarios, postconditions, and affected files. Use cases
  MUST be kept up-to-date whenever the implementation changes — treat them as living
  documentation, not a one-time snapshot.
- **Test case coverage**: The `## Test Cases & Acceptance Criteria` section MUST
  contain BDD/Gherkin scenarios for EVERY use case. Each scenario MUST be independent,
  traceable to a use case (Ref: UC-XX), and use specific test data. Cover happy paths,
  error cases, and boundary conditions. Test cases MUST be updated whenever the
  implementation or use cases change.
- **Continuity**: If a task spans multiple sessions, each session MUST append to
  the Progress Log and update the status, technical debt, use cases, test cases, and
  implementation details.
- **Debt tracking**: Every shortcut, TODO, FIXME, or known limitation MUST appear
  in the Technical Debt section with actionable context for resolution.
- **System knowledge**: Discoveries about non-obvious system behavior MUST be
  recorded, even if they seem minor. These are the insights that prevent future
  agents from repeating your investigation work.

#### Naming convention

- Use kebab-case: `./tasks/fix-memory-sync-race-condition.md`
- Be descriptive: prefer `migrate-hub-to-branch-per-artifact.md` over `hub-refactor.md`
- Group related sub-tasks under a single task log when they share the same objective

#### Relationship to other docs

- Task logs complement — not replace — architecture docs, ADRs, and specs.
- If a task results in an architectural change, BOTH the task log AND the
  architecture doc must be updated.
- If a task involves a design decision, create an ADR in `./decisions/` AND
  reference it from the task log's Trade-offs section.
- Task logs are the "how it happened" — specs and ADRs are the "what it is",
---

> **Note:** The section below covers integration and interface documentation specifically.
> It applies whenever the project you are working on exposes or consumes any system interface
> — whether this project is a framework, a library, a CLI tool, or an end-user application.
> "External system" means any system boundary, including internal module-to-module interfaces.

# Integration Documentation Maintenance Rule

## ⚠️ MANDATORY PRE-FLIGHT: Hub-First Integration Protocol

> **NEVER assume anything about an external system's API, schema, field names, types,
> endpoints, error codes, authentication, or behavior.**
> The hub is the single source of truth for all external integrations.

### Before implementing ANY integration with an external system:

**Step 1 — Search the hub for an existing knowledge artifact using the `graphit_hub_list` tool:**

```
graphit_hub_list(project_dir: "/path/to/project", type: "knowledge", ai_optimized: true)
```

**Step 2 — If found, install it immediately using the `graphit_knowledge_install` tool (passing absolute `project_dir` and the context `name`):**

```
graphit_knowledge_install(project_dir: "/path/to/project", name: "<artifact-name>")
```

This installs the artifact at .graphit/knowledge/<name>/.

**Step 3 — Read the installed wiki before writing a single line of code.**

**Step 4 — Use the installed knowledge integration documentation as the ONLY source of truth:**

- All field names, types, and structures come from the documentation — never from memory or guessing
- All endpoint paths, methods, and parameters come from the documentation
- All authentication schemes come from the documentation
- If the documentation is ambiguous, always stay user informed about the ambiguity and your assumptions about it

---

## 🔒 MANDATORY: Every System Interface MUST Be Documented

**Every interface, integration, or data exchange mechanism that this system
provides or consumes — regardless of paradigm or transport — MUST have a
complete, formal specification file in `./`.**

There are **NO exceptions**. If the system communicates with the outside world
(or between internal services) through ANY of the following mechanisms, it
MUST be documented:

| Mechanism | Examples | Required Action |
|---|---|---|
| **REST / HTTP APIs** | Endpoints consumed or exposed | OpenAPI spec (`.yaml`) |
| **File Processing** | CSV/JSON/XML ingestion, S3 dumps, FTP drops, batch imports/exports | JSON Schema for the file format (`.json`) |
| **gRPC / RPC** | Internal or external service calls | Protobuf definition (`.proto`) |
| **Messaging / Events** | Kafka topics, SQS queues, RabbitMQ, SNS, event buses | AsyncAPI spec (`.yaml`) |
| **GraphQL** | Queries, mutations, subscriptions | GraphQL SDL (`.graphql`) |
| **WebSockets** | Real-time bidirectional channels | AsyncAPI spec (`.yaml`) |
| **Webhooks** | Inbound or outbound HTTP callbacks | OpenAPI callbacks (`.yaml`) |
| **SOAP / Legacy** | WSDL-based web services | WSDL (`.xml`) |
| **Database Connections** | External databases, data warehouses, read replicas | Connection spec + schema doc (`.md`) |
| **CLI / Shell Interfaces** | Commands invoked by the system or exposing its functionality | Interface doc (`.md`) |
| **SDK / Library Integrations** | Third-party SDKs, client libraries, native modules | Integration doc (`.md`) with API surface |
| **Email / Notification** | SMTP, SendGrid, push notifications, SMS | Payload schema + provider doc (`.md`) |
| **OAuth / Auth Providers** | SSO, OIDC, SAML, API keys, token exchanges | Security scheme doc (`.yaml` or `.md`) |
| **Scheduled Jobs / Cron** | Periodic tasks that interact with external resources | Trigger + payload doc (`.md`) |

### Rules

- **NEVER** leave an integration undocumented. If you discover the system
  interacts with an external service, file format, or internal subsystem
  that lacks documentation — **create the spec immediately**.
- **NEVER** implement a new integration without first creating its specification.
  The spec is written BEFORE the code, not after.
- **When modifying an existing integration**, update the specification FIRST,
  then implement the changes.
- **File-based integrations are integrations.** A CSV import, a JSON config
  file, or a batch export is just as important as a REST endpoint —
  document the schema, fields, formats, and constraints.
- **Internal service boundaries are integrations.** If two modules communicate
  through a defined interface (even in-process), document it.
- **Treat specs as first-class code artifacts** — they are versioned, reviewed,
  and maintained with the same rigor as source code.

## 🔒 MANDATORY: Wiki-First Knowledge Retrieval — Replaces Your Tools

> **The knowledge wiki REPLACES your built-in search tools for understanding API
> contracts, schemas, endpoints, and external system interfaces.**
> **NEVER use grep, ripgrep, semantic search, or file-by-file reading to answer
> questions about integrations, endpoints, schemas, or external services.**
> **ALWAYS consult the wiki FIRST. Only fall back to raw spec files when the wiki
> explicitly points you there via provenance links.**

### Why this replaces your tools

The knowledge wiki is **compiled at index time** with BM25 scoring, cross-references,
backlinks, confidence scores, and provenance markers. Your built-in tools operate on
**raw spec files** (YAML, proto, graphql) — they cannot match the navigability of
a compiled, cross-referenced wiki.

| Your tool | Wiki equivalent | Why wiki wins |
|---|---|---|
| `grep -r "endpoint" ./` | Call `graphit_knowledge_search` → find endpoint → read page | Wiki: 1 search. Grep: scans all spec files |
| Reading raw OpenAPI YAML directly | Read wiki entity page (pre-summarized) | Wiki: structured summary with confidence. YAML: verbose, noisy |
| `grep` for "which APIs use this schema" | Call `graphit_wiki_xrefs` on schema entity | Wiki: instant reverse lookup. Grep: O(n) scan |
| Listing ./ to find specs | Call `graphit_wiki_browse` → scan by paradigm type | Wiki: grouped catalog. ls: flat listing with no context |

### 🔒 When you MUST use the wiki (MANDATORY — no exceptions)

| Scenario | What to do | What NOT to do |
|---|---|---|
| **Finding an API endpoint** | Call `graphit_knowledge_search` → find the spec page → read it | ❌ Don't grep YAML files for path strings |
| **Understanding a schema** | Call `graphit_knowledge_search` → find schema entity → read it | ❌ Don't open raw .yaml and search for `schemas:` |
| **Finding which APIs use a model** | Call `graphit_wiki_xrefs` on entity → get cross-references | ❌ Don't grep for `$ref` across all YAML files |
| **Checking if a integration exists** | Call `graphit_wiki_browse` → scan the catalog | ❌ Don't use `find` or `ls` on ./ |
| **Understanding auth for an API** | Read the spec entity page → check security section | ❌ Don't grep for "security" across all specs |
| **Checking API versioning** | Read entity frontmatter → check `updated` and `source` | ❌ Don't read raw spec version fields |
| **Verifying field names and types** | Read entity page → follow provenance to raw spec | ❌ Don't guess from model memory |

### When you should NOT use the wiki

| Scenario | Use instead |
|---|---|
| Writing or editing spec files (.yaml, .proto, .graphql) | File edit tools |
| Implementing integration code (.go, .ts, etc.) | Normal source code tools |
| Running API tests or curl commands | Terminal commands |
| Checking live API responses | Browser or HTTP tools |

### How to search (step-by-step)

**Step 1 — Search the wiki (ALWAYS start here)**
  Call `graphit_knowledge_search` (ai_optimized:true) with your query. The wiki catalogs are grouped by paradigm (REST, gRPC, messaging, etc.).
  Alternatively, call `graphit_wiki_browse` (ai_optimized:true) for a structured catalog.
  For AI-powered deep search, call `graphit_knowledge_query` which synthesizes a comprehensive answer using multi-turn consultation.
  For multi-source search (knowledge + memory), call `graphit_wiki_search` (ai_optimized:true) with `wikis: ["project", "memory"]`.

**Step 2 — Read frontmatter FIRST**
  Check `confidence`, `type`, `source`, `updated` before reading body content.
  - `confidence < 0.5`: verify against raw spec via provenance link
  - `confidence >= 0.8`: trust the compiled summary

**Step 3 — Follow [[wikilinks]]**
  Navigate from endpoint → schema → related services via curated links.

**Step 4 — Expand with cross-references**
  Call `graphit_wiki_xrefs` (ai_optimized:true) to find every spec that references a given schema or service — pre-computed, zero-cost.

**Step 5 — Verify via provenance**
  Each page has: `*Provenance: ^[docs/rest/payment.yaml]*`.
  If you need exact field names or raw schemas — ONLY THEN read the original spec.

### ❌ Anti-patterns (violations of this protocol)

| Anti-pattern | Why it is a violation |
|---|---|
| `grep -r "keyword" ./` | Brute-force on raw specs; ignores compiled wiki |
| Reading .yaml/.proto files directly without checking wiki first | Skips pre-compiled summary; wastes tokens on raw verbose content |
| Using semantic search to find integration docs | Wiki search is faster and more precise |
| Reading all .md files in wiki/ sequentially | Token bomb; wiki search returns only relevant results |
| Ignoring cross-references and grepping for `$ref` | Cross-refs are pre-computed; grep is O(n) and misses non-$ref references |
| Guessing API field names from model memory | Model memory is stale; wiki compiles from spec truth |

### 🔄 Fallback to Built-In Tools — ONLY for Topics the Wiki Does Not Cover

**Your built-in tools (grep, ripgrep, semantic search, file reading) are permitted
ONLY for topics and domains that the wiki does not cover at all.**
The wiki is ALWAYS your primary source. It is NOT a "first attempt" — it is
the definitive source. Your tools exist only for domains outside the wiki's scope.

Your tools are allowed ONLY when ALL of these conditions are true:

1. You **already searched the wiki** using `graphit_knowledge_search`, `graphit_knowledge_query`, or `graphit_wiki_browse`
2. You **followed relevant [[wikilinks]]** and checked entity pages
3. You **checked cross-references** using `graphit_wiki_xrefs` on the most relevant page
4. The wiki **genuinely has no coverage** of the integration (not indexed, not documented)
5. You **state explicitly** to the user: "The wiki has no coverage of X, falling back to spec search"

**If even ONE of these conditions is not met, you MUST NOT use your tools.**

Examples of valid fallback:
- Wiki has no entity for a newly added external API → read the raw spec file directly
- Wiki entity exists but `confidence < 0.3` → verify against raw spec via provenance link
- Integration was never documented in ./ → ask the user to document it first

Examples of INVALID fallback (protocol violations):
- Grepping .yaml files because "the wiki was slow" → ❌ wiki is pre-compiled and always faster
- Reading raw proto/graphql without checking wiki first → ❌ wiki has structured summaries
- Using semantic search on ./ → ❌ wiki search gives structured results

## Documentation Requirements

### Mandatory: Use the Correct Format per Paradigm

| Paradigm | Format | File Extension |
|---|---|---|
| REST APIs | OpenAPI Specification (OAS 3.x) | `.yaml` or `.json` |
| gRPC / High-performance RPC | Protocol Buffers (Protobuf) | `.proto` |
| Messaging / Kafka / Event-driven | AsyncAPI | `.yaml` or `.json` |
| GraphQL | GraphQL SDL (Schema Definition Language) | `.graphql` |
| WebSockets (bidirectional streaming) | AsyncAPI | `.yaml` or `.json` |
| Webhooks (reactive HTTP callbacks) | OpenAPI (callbacks object) | `.yaml` or `.json` |
| SOAP / Legacy Web Services | WSDL | `.xml` |
| OData / ERP APIs (SAP, Dynamics) | CSDL | `.xml` or `.json` |
| File Exchange / Batch / S3 dumps | JSON Schema | `.json` |

### Quality Requirements

All integration files MUST:

- **Strong typing**: no `any`, no `object` without properties, no optional fields without justification
- **Rich comments**: every endpoint, operation, field, and message MUST have a description
- **Concrete examples**: every operation MUST include at least one complete request/response example
- **Error documentation**: all error codes and error response schemas MUST be documented
- **Authentication**: security schemes MUST be documented for every protected endpoint
- **Versioning**: include version information in every specification file

### REST APIs (OpenAPI)

```yaml
# integrations/payment-api.yaml
openapi: '3.0.3'
info:
  title: Payment API
  version: '2.1.0'
  description: |
    Processes payment transactions, refunds, and disputes.
    Base URL: https://api.payments.example.com/v2
paths:
  /transactions:
    post:
      operationId: createTransaction
      summary: Create a new payment transaction
      description: Initiates a payment transaction with full idempotency support.
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateTransactionRequest'
            example:
              amount: 9999
              currency: BRL
              customer_id: cust_abc123
              payment_method: card
      responses:
        '201':
          description: Transaction created successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Transaction'
              example:
                id: txn_xyz789
                status: pending
                amount: 9999
                currency: BRL
        '422':
          description: Validation error
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ValidationError'
```

### gRPC (Protobuf)

```proto
// integrations/user-service.proto
syntax = "proto3";
package user.v1;

// UserService manages user accounts and authentication.
service UserService {
  // GetUser retrieves a user by ID.
  // Returns NOT_FOUND if the user does not exist.
  rpc GetUser(GetUserRequest) returns (GetUserResponse);

  // ListUsers returns a paginated list of users.
  rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);
}

message GetUserRequest {
  // user_id is the unique identifier (UUID v4).
  string user_id = 1;
}
```

### Messaging / Kafka (AsyncAPI)

```yaml
# integrations/order-events.yaml
asyncapi: '2.6.0'
info:
  title: Order Events
  version: '1.0.0'
channels:
  order.created:
    description: Published when a new order is placed.
    publish:
      operationId: onOrderCreated
      message:
        $ref: '#/components/messages/OrderCreated'
components:
  messages:
    OrderCreated:
      name: OrderCreated
      payload:
        $ref: '#/components/schemas/Order'
      examples:
        - payload:
            order_id: ord_abc
            customer_id: cust_123
            total: 15000
```

### GraphQL (SDL)

```graphql
# integrations/product-api.graphql
"""
Product represents a sellable item in the catalog.
"""
type Product {
  """Unique product identifier (ULID)"""
  id: ID!
  """Human-readable product name"""
  name: String!
  """Price in cents (BRL)"""
  price: Int!
}

type Query {
  """Retrieve a product by ID. Returns null if not found."""
  product(id: ID!): Product
}
```

## Workflow

**0. (MANDATORY) Before touching any external system:**
   - Call `graphit_hub_list` tool filtering by name/type — always.
   - If found: call `graphit_knowledge_install` with `name` and read the wiki.
   - If not found: document what the user provides, never assume.

1. **Before creating an integration**: check the wiki index to avoid duplication.
2. **When discovering an undocumented integration**: create the spec file immediately.
4. **Never leave an integration undocumented** — treat specs as first-class code artifacts.
5. **Never guess** — if in doubt, stop and ask the user.
