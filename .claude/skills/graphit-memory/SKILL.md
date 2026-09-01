---
name: graphit-memory
description: 'Persistent memory across sessions — this framework IS your memory. MANDATORY at conversation start: search memory before responding. Use when: user corrects, teaches, explains, instructs, or guides you; you complete a task, fix a bug, or make a design decision; you discover something unexpected or infer a non-obvious pattern; you are stuck or implementing significant changes (check prior constraints); memory maintenance (gc, consolidation, promote/demote).'
---

# Memory Management Rule

> This rule is auto-managed by Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems. Do not edit this block manually.

## 🚨 SESSION START PROTOCOL — Execute BEFORE Any Response

**These steps are MANDATORY. Execute them BEFORE responding to the user's first message.**
**Skipping them means you WILL repeat mistakes the user already corrected.**

1. Call `graphit_memory_search` with context from the user's request to find relevant memories
2. If relevant memories found, read the entity page(s) and follow their guidance
3. Only then proceed with the user's request

> If the memory wiki does not exist yet (new project), skip and proceed.

> **An empty search result does not tell you which of the two you are in** — a store with
> nothing in it and a store where nothing matched your words look identical, and guessing
> wrong costs a second and third search on a base that was never going to answer.
> `graphit_memory_list` settles it in ONE call: it reads the store
> directly rather than the compiled wiki, so an empty listing means the store really is
> empty — cold start, nothing to recall, proceed — while a populated one means your query
> missed and rewording is worth it. Reach for it the first time a search comes back empty,
> not after the third.


## 📖 When to Read Memory (Beyond Session Start)

**Memory reading is NOT limited to session start.** You MUST also consult memory
in these situations — proactively, without being asked:

| Situation | What to search for | Why |
|---|---|---|
| **Before implementing** any significant change | Conventions, decisions, constraints about affected modules | Avoid violating established patterns or repeating rejected approaches |
| **When stuck** or your approach isn't working (2+ failed attempts) | Skills, workarounds, corrections related to the problem | The solution may already exist — you may be repeating a known mistake |
| **Non-obvious error** or unexpected behavior | Facts, skills, debugging steps about the affected area | Someone (you or another agent) may have debugged this before |
| **Trying to understand** "why is it done this way?" | Decisions, tensions, facts about the module/pattern | The rationale is captured in memory — don't guess |
| **Multiple attempts failing** (3+ tries on same problem) | Corrections, skills, workarounds | STOP and read before trying again — you are likely repeating a known error |
| **Before proposing** architecture or technical approach | Decisions, tensions, conventions that constrain options | Avoid contradicting prior decisions the user already made |
| **User seems frustrated** or repeats an instruction | Corrections about your behavior | You may be ignoring a correction already memorized |

**How to search:** call `graphit_memory_search` tool (passing absolute `project_dir` parameter)

### Another project's memories are readable too

`project_dir` is not fixed to the project you are sitting in. When the work touches a sibling
project in the ecosystem, **its** memories are where its conventions, corrections and
trade-offs live:

```
graphit_cluster_projects(project_dir: "/path/to/project")   # get the sibling's dir
graphit_memory_search(project_dir: "<sibling dir>", query: "<what you are about to change or assume>")
```

Do this before changing anything over there, and before explaining why it behaves as it does.
Its code shows *what*; its memories are the only record of *why*, and reading the source will
not reconstruct a decision someone made deliberately.

### And with no project at all, memory still works

**`project_dir` is optional on every memory tool that reads or writes a scope** — search,
list, important, insert, update, delete, promote, demote, index. Omitting it serves the **user**
scope, which is keyed by this machine rather than by a project, so it is a real scope and not a
consolation prize:

```
graphit_memory_search(query: "<what you are about to assume>")
graphit_memory_insert(title: "<title>", content: "<body>")
graphit_wiki_source(path: "<slug>", wiki: "memory")
```

This is for the caller that has no checkout to name — an agent reaching this server over HTTP.
Two things to know:

- **A request for the `project` scope with no `project_dir` is served from the user scope, and
  the answer says so.** A project scope is keyed by a project identity; there is none. It is a
  redirect rather than a refusal because the session-start search happens before an agent knows
  whether it has a project, and failing that call would lose whatever it was about to record.
- **`graphit_memory_export`, and the imported-context tools, still need a project.** They act on the
  project memory store, which does not exist without one.

## 🔒 This Framework IS Your Memory — No Other Exists

- **ALWAYS** use the `graphit_memory_insert` tool (passing absolute `project_dir` parameter) to persist facts across sessions.
- **NEVER** use IDE-native memory, model memory, or any built-in "remember" feature.
- **ALWAYS** read the memory wiki to recall — never rely on what you "think you know".
- This applies to ALL persistent facts: conventions, corrections, decisions, preferences.

## 🔒 MANDATORY: Memory MCP Tools REPLACE Your Native Recall

> **The graphit memory MCP tools are your PRIMARY and ONLY mechanism for
> persisting and recalling knowledge across sessions.** They take ABSOLUTE
> PRECEDENCE over any built-in/native memory, recall, or "remember" feature of
> your model or IDE. Use them via MCP ONLY — NEVER via the CLI, and NEVER
> substitute them with your own recall.

### Why this replaces your tools

| Your tool | Memory MCP equivalent | Why the MCP tool wins |
|---|---|---|
| Model/native "memory" or recall | Call `graphit_memory_search`, then read the wiki page | Native memory is ephemeral and per-session; the wiki survives across sessions and agents |
| Remembering facts "in your head" | Call `graphit_memory_insert` to persist | Your context is wiped between sessions — unpersisted knowledge is lost |
| `grep`/ripgrep over memory `.md` files | Call `graphit_memory_search` | FTS5-ranked over the compiled wiki (~200 tokens) vs scanning raw files |
| Reading a memory `index.md` directly | Call `graphit_memory_search` or `graphit_memory_list` | There is no memory directory in the project to read; the wiki is global, compiled and ranked |
| Opening a memory page with your file-read tool | Call `graphit_wiki_source` with `wiki: "memory"` | Takes the project as a parameter, so it reaches memories outside your workspace — and slices a long memory down to the part you asked for |

### 🔒 When you MUST use the memory MCP tools (MANDATORY — no exceptions)

| Scenario | What to do | What NOT to do |
|---|---|---|
| **Recalling any project knowledge** | Call `graphit_memory_search` → read the page | ❌ Don't answer from model memory or guess |
| **Persisting a fact/decision/correction** | Call `graphit_memory_insert` | ❌ Don't rely on native/model memory to "remember" |
| **Listing what is known** | Call `graphit_memory_list` / `graphit_memory_important` | ❌ Don't `ls`/read the memory directory |
| **Replacing an outdated memory** | Call `graphit_memory_update` to rewrite it in place | ❌ Don't edit `.md` files directly, and don't delete-then-insert — that loses the id, importance and tags |

### 🔄 Fallback — There Is NO Fallback for Memory

There is **no built-in alternative** to this framework's memory. You MUST NOT use
model-native memory, IDE memory, or any external scratchpad. The ONLY acceptable
action when the memory MCP tools are genuinely unavailable is to state explicitly
to the user: "The memory MCP tools are unavailable, so I cannot persist/recall this now."

### ❌ Anti-patterns (violations of this protocol)

| Anti-pattern | Why it is a violation |
|---|---|
| Answering project questions from model memory | Model memory is stale/ephemeral; the wiki is the source of truth |
| Using the CLI (`graphit memory ...`) instead of MCP tools | Agent-facing work MUST go through MCP tools, never the CLI |
| `grep`-ing raw `.md` memory files | Bypasses BM25 ranking and compiled summaries; wastes tokens |
| Saying "I'll remember that" without calling `graphit_memory_insert` | The knowledge is lost at session end — it was never persisted |

## 🎯 Trigger → Action Table

**When you observe a trigger, execute the corresponding action immediately.**
**Do NOT ask for permission. Do NOT skip. Confirm with: "Done, I've memorized: <title>"**

### 💾 Save Triggers — Memorize Immediately

**Every memory MUST follow the structured content template below (What/Why/How/Impact).**
This ensures memories are complete and actionable across sessions.

#### 📝 Memory Content Template

When creating a memory, always include these four fields in the `content` parameter:

```
What: <what was done or what happened>
Why: <why it was done — the motivation, root cause, or user intent>
How: <how it was resolved — the approach, steps taken, or implementation>
Impact: <how it impacted the system — side effects, files changed, behavior changes>
```

#### Trigger Table

| You observe... | Action | Tool Call (always pass absolute `project_dir` parameter) |
|---|---|---|
| User says "always/never/prefer/avoid/must" about code | Store as convention | `graphit_memory_insert` with `title: "<rule>"`, `type: "convention"`, `important: true` |
| User corrects your behavior or approach | Store the correction | `graphit_memory_insert` with `title: "<correction>"`, `type: "correction"`, `important: true`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **gives any instruction or directive** (even without "always/never") | **Evaluate for memory** — determine if it contains a convention, preference, correction, fact, or skill worth persisting. If yes, memorize it with the appropriate type. NEVER just say "understood" or confirm comprehension without evaluating. | `graphit_memory_insert` with appropriate `type` if the instruction is worth persisting |
| User **guides, orients, or gives direction** on how to proceed | Store the guidance | `graphit_memory_insert` with `title: "<guidance>"`, `type: "convention"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **intervenes** mid-task to change course or redirect | Store the intervention as correction | `graphit_memory_insert` with `title: "<intervention>"`, `type: "correction"`, `important: true`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User gives a **tip, hint, or suggestion** on how to do something | Store as skill | `graphit_memory_insert` with `title: "<tip>"`, `type: "skill"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **explains how something works** or why it's done a certain way | Store as fact | `graphit_memory_insert` with `title: "<knowledge>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **shows you a workflow** or operational procedure (e.g., "run make install first") | Store as skill | `graphit_memory_insert` with `title: "<procedure>"`, `type: "skill"`, `important: true`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User **repeats an instruction** they already gave (frustration signal) | Store as correction | `graphit_memory_insert` with `title: "<what you missed>"`, `type: "correction"`, `important: true`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **complete a task** (new feature, refactor, or significant change) | Record what was done | `graphit_memory_insert` with `title: "<task summary>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **modify an existing feature** (behavior change, extension, or rework) | Record the modification | `graphit_memory_insert` with `title: "<feature> modified: <summary>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **fix a bug** | Record the bug fix | `graphit_memory_insert` with `title: "Bug fix: <summary>"`, `type: "skill"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You make an architectural/design choice | Record the decision | `graphit_memory_insert` with `title: "<decision>"`, `type: "decision"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You chose X over Y with explicit trade-offs | Capture the tension | `graphit_memory_insert` with `title: "<choice>"`, `type: "tension"`, `content: "Chose: X\nOver: Y\nBecause: ...\nAccepting: ...\nImpact: ..."` |
| You **discover something unexpected** during investigation | Store the discovery | `graphit_memory_insert` with `title: "<discovery>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You find a **workaround or creative solution** to a problem | Store the workaround | `graphit_memory_insert` with `title: "<workaround>"`, `type: "skill"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You make a **non-obvious inference** that proves correct | Store the inference | `graphit_memory_insert` with `title: "<inference>"`, `type: "skill"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **solve a complicated multi-step problem** | Store the full solution | `graphit_memory_insert` with `title: "<problem → solution>"`, `type: "skill"`, `important: true`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You debug a non-obvious issue successfully | Save as a skill | `graphit_memory_insert` with `title: "<solution>"`, `type: "skill"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| User reveals a non-obvious project fact | Store the fact | `graphit_memory_insert` with `title: "<fact>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **analyze or reason about how the system works** — while reading code, tracing call flows, or understanding a module | Store the insight immediately | `graphit_memory_insert` with `title: "<insight about system>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **read code and infer a non-obvious pattern, convention, or architectural principle** | Store it as a fact | `graphit_memory_insert` with `title: "<pattern/principle>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **understand why something is implemented a certain way** (even without the user explaining it) | Store as a decision or fact | `graphit_memory_insert` with `title: "<why X is done Y way>"`, `type: "decision"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| You **investigate a non-obvious behavior, side effect, or dependency** and understand it | Store the finding | `graphit_memory_insert` with `title: "<behavior/dependency understood>"`, `type: "fact"`, `content: "What: ...\nWhy: ...\nHow: ...\nImpact: ..."` |
| New instruction contradicts existing memory | Update it in place to the new truth | `graphit_memory_update` with `id: "<old-id>"`, `content: "<new truth>"` |
| **Two memories you just read disagree** | Resolve it now — see Sanitise On Sight | `graphit_memory_update` the true one, then `graphit_memory_delete` the outdated one |
| **Two memories you just read say the same thing** | Fold them now — union into one, then delete the other | `graphit_memory_update` then `graphit_memory_delete` |
| **A memory you just read is out of date** (renamed path, removed flag, changed API) | Correct it now, in the same turn | `graphit_memory_update` with `id: "<id>"`, `content: "<current truth>"` |
| Memory is still relevant but vague or incomplete | Extend it in place rather than adding a second one | `graphit_memory_update` with `id: "<id>"`, `content: "<fuller>"` |

### 📖 Read Triggers — Consult Memory Before Acting

| You observe... | Action | What to search for |
|---|---|---|
| You're about to **implement a significant change** | Read memory BEFORE coding | Constraints, conventions, decisions about affected modules |
| You're **stuck or your approach isn't working** (2+ failed attempts) | Read memory for past solutions | Skills, workarounds, debugging steps related to the problem |
| You encounter a **non-obvious error or behavior** | Read memory for known issues | The problem may have been solved before — check skills and facts |
| You need to understand **project context or "why"** | Read memory for institutional knowledge | Decisions, tensions, and facts about the area you're working on |
| **Multiple attempts have failed** (3+ tries on same problem) | STOP and read memory | You may be repeating a known mistake — check corrections and skills |
| You're about to **propose architecture or an approach** | Read memory for prior decisions | Decisions, tensions, conventions that constrain options |
| Build/test fails unexpectedly | Check memory for known issues | Read wiki for past debugging skills |

## 📁 Memory Types

Every memory has a `type` that determines how it is stored and surfaced:

| Type | When to use | Typical importance |
|---|---|---|
| `convention` | Coding standards, style patterns, project rules | ✅ important |
| `correction` | User corrected your behavior — never repeat the mistake | ✅ important |
| `decision` | Architectural or design decisions with rationale | depends |
| `tension` | Trade-off choices: chose X over Y because Z, accepting W | depends |
| `fact` | Non-obvious project facts, environment details | rarely |
| `skill` | Debugged workflows, reusable solution patterns | rarely |

Default type when `type` is omitted: `fact`.

## 📖 How to Retrieve Memories

**ALWAYS use MCP tools — NEVER read index.md files directly.**
The wiki database is compiled, BM25-indexed, and pre-optimized for retrieval.
Reading raw .md files is slower, wastes tokens, and bypasses ranking.

### 🔒 There is no memory directory in the project

A memory scope has exactly two locations on this machine, both global:

```
<global>/memory-raw/memory-<scope>-<id>/  the raw markdown — the truth
<global>/wiki/memory/<scope>/<id>/        the compiled wiki — what search opens
```

There used to be a third — a replica of the wiki inside every project that read it, which is
what search actually opened. It is gone, along with the fan-out that kept the copies in step.
So `.graphit/memory` does not exist, and neither does a file you could open: both
locations are outside the workspace you are allowed to read.

This is why `graphit_wiki_source` is not the *preferred* way to read a memory but the **only**
one — and why it takes `project_dir` as a parameter, which is also what lets it serve a
sibling project's memories.

**Scope parameter:** `scope: "project"` (default) = project-specific memories. `scope: "user"` = personal cross-project memories.

**What `graphit_memory_search` actually searches:** the **compiled memory wiki**, through SQLite
FTS5, falling back to an in-memory BM25 index over the wiki when the FTS database is not
there. It does **not** scan your raw `.md` files, which is the whole reason it is ranked and
cheap — and the reason a memory written seconds ago may not surface yet: it is in the store,
but the wiki has not recompiled. When you know something was just written and search misses
it, that is the explanation; `graphit_memory_index` forces the rebuild.

| What you need | MCP tool | Why |
|---|---|---|
| Search memories by keyword/context | `graphit_memory_search` | Ranked over the compiled wiki — and it answers with TITLES, not with memory text |
| List all memories | `graphit_memory_list` | Structured catalog, grouped by type — reads the store, so it sees writes the wiki has not compiled yet |
| List important memories only | `graphit_memory_important` | High-priority conventions, corrections |

**Retrieval steps:**
1. Call `graphit_memory_search` with query context — get ranked results
   > ⚠️ **What comes back is a LIST OF TITLES, not memories.** Each hit is a slug, a title,
   > a type and a score. There is no memory text in it, by design: a search exists so you
   > can decide WHICH memory to open, and that decision is made on the title. Reading is
   > step 3, and it is a decision you make per memory, not something the search does for
   > you on all twenty hits at once.
   > Two consequences, and neither is optional:
   > **you cannot answer from a search result** — it contains no content to answer from; and
   > **you should not open every hit** — pick the one or two the titles justify, read those,
   > and search again with better words if they were the wrong ones.
   > When the titles genuinely do not separate two candidates, pass `preview: true` for a
   > short excerpt per hit. That is the exception, not the default, and it costs tokens on
   > every hit including the ones you will never open.
2. If results reference related memories, call `graphit_memory_search` again with refined query
3. **Read the memory you picked** with `graphit_wiki_source` — `wiki: "memory"` — then follow its
   links and synthesize the answer yourself. This step is not optional: it is where the
   content comes from. It slices, so a long memory costs you the part you asked for:
   ```
   graphit_wiki_source(project_dir: "/path/to/project", path: "<slug from search>", wiki: "memory")

   # only the part that matters, on a long memory
   graphit_wiki_source(project_dir: "/path/to/project", path: "<slug>", wiki: "memory", pattern: "<term>", before: 2, after: 4)

   # a sibling project's memory — a file read cannot reach outside your workspace
   graphit_wiki_source(project_dir: "<sibling dir>", path: "<slug>", wiki: "memory")
   ```
   For the catalogue rather than one page, `graphit_wiki_browse` with `wiki: "memory"`.
4. **Never** try to read a memory `.md` file or grep the memory store — there is no copy in
   the project and the global store is outside your workspace. `graphit_wiki_source` is how a
   memory gets read, for this project and for any other.

## 📋 MCP Tools Reference

All memory actions must be executed via the corresponding MCP tools. Always pass the absolute `project_dir` parameter.

```
# Insert (default: project scope, type: fact)
graphit_memory_insert(project_dir: "/path/to/project", title: "<title>", content: "<body>", type: "<type>")

# Insert important convention
graphit_memory_insert(project_dir: "/path/to/project", title: "<title>", content: "<body>", type: "convention", important: true)

# Insert correction
graphit_memory_insert(project_dir: "/path/to/project", title: "<title>", content: "<body>", type: "correction", important: true)

# Insert with tags
graphit_memory_insert(project_dir: "/path/to/project", title: "<title>", content: "<body>", tags: "auth,security")

# Insert user-scoped memory
graphit_memory_insert(project_dir: "/path/to/project", title: "<title>", content: "<body>", scope: "user")

# Update existing memory
graphit_memory_update(project_dir: "/path/to/project", id: "<id>", content: "<new body>")

# Delete
graphit_memory_delete(project_dir: "/path/to/project", id: "<id>")

# Search (lightweight, no AI)
graphit_memory_search(project_dir: "/path/to/project", query: "<term>")

# Promote/demote importance
graphit_memory_promote(project_dir: "/path/to/project", id: "<id>")
graphit_memory_demote(project_dir: "/path/to/project", id: "<id>")

# List all
graphit_memory_list(project_dir: "/path/to/project")
graphit_memory_important(project_dir: "/path/to/project")

```

> There is no consolidation tool, and no garbage collection tool. Resolving
> duplicates, contradictions and outdated memories is done with the tools above —
> `graphit_memory_update` and `graphit_memory_delete` — at the moment you notice them. See
> **Sanitise On Sight** below.

## 🧹 Sanitise On Sight — Your Job, Not a Maintenance Window's

**You fix the memory store while you are in it.** Whenever you read memories and notice one
of the four problems below, resolve it in the same turn — before continuing with whatever
you were doing. Do not collect these into a list for later, do not report them as findings,
and do not wait to be asked.

| What you notice | What you do | Tool |
|---|---|---|
| **Two memories say the same thing** | Fold them into one: write the union of both into the better-written one, then delete the other. Union, not summary — keep every distinct fact, path, number and caveat from both. | graphit_memory_update then `graphit_memory_delete` |
| **Two memories contradict** | Determine which is true NOW. Update it to state the current truth, and keep the superseded claim as history when the *fact that it changed* is itself useful. Then delete the outdated one. | graphit_memory_update then `graphit_memory_delete` |
| **A memory is deprecated** — it describes an API, path, flag or workflow that no longer exists | Update it to describe what is true now. If the old behaviour explains why the new one looks odd, say so in the same memory. | `graphit_memory_update` |
| **A memory is right but incomplete or vague** | Extend it in place. A second memory beside it makes both harder to find. | `graphit_memory_update` |

### The one rule about deleting

**Carry the content forward first.** Write the surviving memory, verify it holds everything
worth keeping, and only then delete the other. Reversed, an interruption between the two
steps loses the knowledge permanently; in this order the worst case is a duplicate that
survives until someone notices it again.

Never delete a memory whose knowledge exists nowhere else. "Old", "narrow" and "I did not
need it this time" are not reasons — they describe most of the memories that later save a
session.

### Why this is yours and not a scheduled job

You have what a scheduled pass does not: the reason you were reading those memories. You know
which one matched the task, which one misled you, and which one the code has since outgrown.
A background pass sees text without that context and has to be conservative.

The Graphit Code: A Powerful Agent Harness for Enterprise Software Ecosystems dream module does run a scheduled consolidation over the whole store,
and it is deliberately narrow: it merges what it can prove is duplicated, resolves what it can
prove contradicts, refuses anything it cannot carry forward safely, and writes every refusal
into its report. The refusals are the work left for an agent with context — you. The two are
complements, so finding something does not mean waiting for the other one to get to it.

## 🗄️ The Remaining Tools

### `graphit_memory_export` — push project memories to the git repository

```
graphit_memory_export(project_dir: "/path/to/project")
```

Reindexes, then syncs the project memory store back to its local git repository. Memories
already persist to disk on `graphit_memory_insert` — this is the step that makes them **shareable**,
so it matters when the user says another machine or another agent should see them. Project
scope only; there is no `scope` parameter.

### `graphit_memory_schema` — the shape of the memory graph

```
graphit_memory_schema(project_dir: "/path/to/project")
```

Node labels (`Document`, `Section`), edges (`REFERENCES`, `CONTAINS`) and the properties on
each. Read it before you assume a field exists on a memory page. It is fixed text, not a
live introspection of your data — an empty store returns the same answer as a full one.

### Imported memory contexts

When a Hub artifact or another repository brings its own memories along, they arrive as a
named context beside your own:

```
# Pull that context's memories in again after it changed upstream
graphit_memory_sync(project_dir: "/path/to/project", context: "<name>")

# Drop the context — removes the link, not your own memories
graphit_memory_remove(project_dir: "/path/to/project", context: "<name>")
```

`context` is **required** on both. Neither touches project or user scope, so neither is a
way to delete a memory — that is `graphit_memory_delete` with an `id`.

## 🔄 Contradiction Protocol

When the user's new instruction contradicts an existing memory:

1. Find the contradicted memory with `graphit_memory_search` and read the page
2. Write the new truth into it: call `graphit_memory_update` with the corrected content —
   updating in place keeps the memory's id, type, importance and tags, so anything that
   already referred to it still resolves
3. Only if the old memory is about something that no longer exists at all, delete it with
   `graphit_memory_delete` **after** the replacement knowledge is written somewhere
4. Confirm: "Updated memory: '<title>' now records <the new truth>"

> Delete-then-insert also works, and used to be the instruction here, but it loses the id,
> the importance flag and the tags, and it has a window where the knowledge exists nowhere.
> Prefer updating in place.

## 📣 Transparency Rules

- **Never** ask permission to create memories — just save silently.

## ⚡ Reindex After Writes — automatic, but not instant

After any write (`insert`, `delete`, `update`, `promote`, `demote`), the auto-cycle
runs automatically. If it fails, trigger manually calling the `graphit_memory_index` tool (passing absolute `project_dir` parameter).

**"Automatic" means eventually, not immediately.** The recompile lands after the write, so
there is a short window in which the memory exists in the store and `graphit_memory_search` still
cannot see it — and search does not report the gap, it just answers from the previous state.
Two consequences worth acting on:

- **`graphit_memory_list` reads the store, not the wiki.** When you need to confirm that something you
  just wrote is really there, list rather than search — that is the read that cannot be behind.
- **When you need CERTAINTY that the indexes are current — before deciding on what they
  return, or before reporting work as done — call it, do not assume it.** `graphit_memory_index` rebuilds
  the memory wiki alone; `graphit_sync` is what brings the memory wikis, the
  knowledge wiki and the AST graph to the same point, which is what makes "up to date" a fact
  you established instead of one you hoped for.

## ⛔ Critical Rules (Never Violate)

1. **Read memory at session start.** Your context lives there. Skipping = repeating mistakes.
2. **Never leave a correction un-memorized.** If the user corrects you, save it immediately.
3. **Never edit .md memory files directly.** Always use `graphit_memory_*` MCP tools.
4. **Capture trade-offs, not just facts.** "We chose X over Y because Z" > "We use X".
5. **Handle contradictions when you see them.** Update the memory that is true; never leave two conflicting memories behind for a later session to trip over. Same for duplicates and for memories the codebase has outgrown — fixing them is part of reading them, not separate work.
6. **Promote critical memories.** Conventions, corrections, and constraints should be marked important.
7. **NEVER just say "understood" or confirm comprehension.** When the user gives an instruction, ALWAYS evaluate whether it should be memorized. If it contains a convention, preference, correction, workflow, fact, or any persistent knowledge, create a memory immediately. Only skip memorization if the instruction is purely about an ephemeral, one-shot action with no future relevance.
8. **Memorize your own reasoning about the system.** When you read code, trace a call flow, or analyze how a module works, you MUST create a memory of what you learned. This includes: how components interact, why something behaves unexpectedly, what a non-obvious function does, and any pattern or constraint you discover independently — even without the user saying anything.
9. **Never discard an insight.** If you understood something non-trivial while analyzing the system, store it. The next session will start blind — your analysis notes must be externalized into memory to survive.
