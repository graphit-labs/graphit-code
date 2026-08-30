---
Title: The Task Log Opens the Task, an Interruption Resumes Framework-Tool Priority, and Sync Is How You Get Certainty
status: done
created: 2026-08-19
updated: 2026-08-19
tags: [mandate, skills, task-log, sync, continuity]
---

The task log opens the task, not closes it.

## Objective

The Engineer requested three new instructions, all in the mandate and skills — i.e., in the document builders that generate `AGENTS.md` and `SKILL.md`, never in the generated `.md` files themselves:

The task log is the first artifact of a task, not the last. At the start of the work, the agent creates `docs/tasks/<task>.md` with the reasoning, the justification, and what will be done broken down into tasks with their own specs. As it makes progress, it updates the log — at every step forward and every change of direction — so that any other agent can pick up from where it left off at any point.

Interruption is not an exemption. When the agent is interrupted, or when the user asks for changes or corrections to the work, on resuming it must prioritize these tools over its own native ones again — reopening the skill for the domain it is returning to.

Automatic synchronization has a small delay. The daemon's watcher reindexes after the write, not with it. Whenever the agent needs to be certain that what a tool returns is current, it calls the MCP sync, which is what brings knowledge, memory, and AST to the same point.

The real problem all three instructions solve is continuity. A log written at the end is a report, not a handover — if the session dies halfway through, there is no record at all. A user correction is exactly the moment the agent is most rushed and most tempted to fall back on grep, and an index that answers right after being written is trusted as if it reflected a state that no longer exists.

## Plan & Task Breakdown

- [x] **T1 — Mandate preamble** (`internal/hub/adapters/ide/mandate.go`, `mandatePreamble`). Spec: two new clauses, global (apply to all five modules, not just one): (a) reiteration after an interruption/correction/change of course applies to the whole mandate; (b) automatic reindexing is asynchronous and lags — when certainty is needed, call `graphit_sync` and let it finish. Constraint: the content must not contain pseudo-tags `<word>`, because `parseTriggers` treats any `<word>` as a tag.
- [x] **T2 — Knowledge module mandate trigger** (`internal/knowledge/rule.go`, `MandateTrigger`). Spec: new triggers for "I'm starting a task," "I finished a step / changed direction," and "I'm resuming interrupted work"; `alwaysClause` now states that the log opens the task.
- [x] **T3 — Knowledge skill** (`internal/knowledge/rule.go`, `KnowledgeRuleContent`). Spec: new section under Task Logs — the log opens before the work, with reasoning, justification, and the plan broken down into task-scoped specs; continuous updates at each step; a resume section (review the log, reopen the skills, only then continue). The "When to create or update" table gains the first row before the task even starts. The full template gets a `## Plan & Task Breakdown` section. A note on the sync delay in the indexing section.
- [x] **T4 — AST skill** (`internal/ast/rule.go`): the mandatory end-of-session sync section explains the delay and what to do when certainty is needed mid-session.
- [x] **T5 — Memory skill** (`internal/memory/rule.go`, "Reindex after writes"): the same delay, plus the choice between `memory_index` (narrow) and a full sync (all three indexes).
- [x] **T6 — Tests tying the three instructions to the generated content**, following the repo's existing pattern (assertions on the content the builders return).
- [ ] **T7 — Regenerate the versioned artifacts** (`AGENTS.md` and the six `SKILL.md` files), which are produced by the installed binary, not the source tree — see System Knowledge. The path chosen by the Engineer in this session: `make install` (sudo password available on this machine), which installs the new launcher — and the launcher re-extracts `graphit-core` and `graphit-mcp` when its BuildID no longer matches `.build-id`/`launcher.stamp`, so no manual runtime swap of the kind described in memory is required.

## Implementation Details

### T1 — `mandatePreamble()`

Two clauses added to the preamble, which is written once and applies to all five blocks:

- **"An interruption is not an exemption."** Being interrupted, corrected, or redirected re-applies the mandate in full on resume: reopen the domain skill, redo the lookups, keep MCP tools ahead of native ones. The text names the reason — urgency is exactly when the agent falls into the grep trap — and the consequence: work already done becomes speculation, because the user just changed the premise.
- **"Automatic indexing lags the change."** The watcher reindexes after the write. A tool called during that window responds with an index that does not yet contain what was just written, and it responds with the same confidence either way. When certainty is needed — before deciding based on a result, before declaring work complete, and after any change that did not come from the agent's own edits (pull, checkout, rebase, restore) — call `graphit_sync` and let it finish. It is the only call that brings knowledge, memory, and AST to the same point.

### T2 — `doc_rule` trigger

Three new triggers appear in the order the situation arises:

- "you are about to start a task of any size — the task log is the FIRST artifact, written
  before the work, not a report you write after it"
- "you finished a step, changed direction, hit a blocker, or learned something that changes
  the plan — the log is updated then, not at the end"
- "you are resuming after an interruption or a correction — read the existing log before you
  touch anything, then continue from it"

And `alwaysClause` now opens with "The task log opens the task," while keeping the older statement that the log is a condition of completion.

### T3 — knowledge skill

- The "When to create or update a task log" table gained a first row: **before starting any task** → create a log with objective, reasoning, and plan, inline.
- New section, "The task log opens the task — it is not a report you write at the end": the minimum content of the opening (objective, reasoning, justification for rejected alternatives), the continuous-update rule (the trigger is the completed step, the changed direction, or the blocker — not the end of the task), and the sufficiency test: if the session ended right now, could another agent continue by reading only the log?
- New section, "Resuming after an interruption or a correction": read the log first, then reopen the framework's skills (a correction does not suspend the mandate), record the change of direction in the Progress Log with the reason, and only then continue.
- The full template gained a `## Plan & Task Breakdown` section with checkboxes and a spec per item, between `## Objective` and `## Implementation Details`.
- The Quick Task Log no longer starts with `status: done` — now it's `in-progress | done`, and it too is created before the change.
- The indexing section got a note on the delay and a distinction between "don't call sync on every edit" and "call sync when you need certainty."

### T4 and T5

The same fact — reindexing is asynchronous — is stated inside each module because that is where the agent is when the delay bites: in AST, next to the mandatory end-of-session sync; in memory, next to the existing note that a memory just written may not yet appear in search results — which now also states what to do when certainty is needed.

## Use Cases

### UC-01: Agent starts a task

- **Actor**: agent in session (any one that receives the mandate).
- **Preconditions**: the mandate is in `AGENTS.md`; the `graphit-knowledge` skill is installed; the user just requested a task.
- **Main Flow**:
  1. The trigger "you are about to start a task of any size" makes the agent open the skill.
  2. The skill instructs it to create `docs/tasks/<task>.md` with frontmatter `status: in-progress`, `## Objective`, reasoning, and `## Plan & Task Breakdown` — one entry per task, each with its own spec.
  3. Only then does the agent edit code.
- **Alternative Flows**:
  - Small change: the Quick Task Log is acceptable, created before the change with `status: in-progress`.
  - A log already exists for this purpose: update it instead of creating another one.
- **Error Scenarios**:
  - Agent writes code first: the skill flags this as a missing log — the task is not complete, regardless of whether the code is ready.
- **Postconditions**: there exists a record readable by another agent before any edit.
- **Affected Files**: `internal/knowledge/rule.go`.

### UC-02: Agent updates the log at every step of progress

- **Actor**: agent in session.
- **Preconditions**: a task log with `status: in-progress` exists.
- **Main Flow**:
  1. The agent completes a plan item, changes approach, or hits a blocker.
  2. It checks off the item, adds to the `## Progress Log` what was done and what comes next, and updates `updated:`.
  3. It continues the work.
- **Alternative Flows**:
  - Change of direction: the old item stays, with the reason for abandoning it recorded — the Progress Log is append-only.
- **Error Scenarios**:
  - Session interrupted without an update: the next agent reads a plan whose state no longer matches the code, which is exactly the scenario this instruction exists to prevent.
- **Postconditions**: the log answers "where it stopped" without needing the conversation history.
- **Affected Files**: `internal/knowledge/rule.go`.

### UC-03: Agent resumes interrupted or corrected work

- **Actor**: agent in session, after a user interruption or correction request.
- **Preconditions**: there was prior work in this or another session.
- **Main Flow**:
  1. The mandate preamble re-applies: the trigger lists hold again, in full.
  2. The agent reopens the skill for the domain it is re-entering and repeats the lookups (memory, wiki, graph) instead of acting on assumptions formed before the interruption.
  3. It reads the existing task log, records the correction and the new direction in the Progress Log, and continues with framework tools ahead of native ones.
- **Alternative Flows**:
  - The correction invalidates part of the plan: affected items are rewritten, not deleted.
- **Error Scenarios**:
  - The agent treats the correction as an extension and falls into the trap of a quick grep/read done in a hurry — precisely what the clause names.
- **Postconditions**: the resumed work follows the mandate exactly as on the first turn.
- **Affected Files**: `internal/hub/adapters/ide/mandate.go`, `internal/knowledge/rule.go`.

### UC-04: Agent needs certainty about the current state of the indexes

- **Actor**: agent in session.
- **Preconditions**: there was a recent write (code, docs, or memory), or the tree changed from outside (pull, checkout, rebase, restore).
- **Main Flow**:
  1. The agent recognizes it is about to decide based on what a tool returns, or about to declare the work complete.
  2. It calls `graphit_sync` with the `project_dir` and lets it finish.
  3. Only then does it read the indexes and draw its conclusion.
- **Alternative Flows**:
  - Only one module is in question: `graphit_knowledge_sync`, `graphit_memory_index`, or the AST equivalent covers that fraction of the work.
- **Error Scenarios**:
  - Without syncing, the response comes from a stale index and is indistinguishable from a correct one — the exact failure mode this instruction names.
- **Postconditions**: knowledge, memory, and AST reflect the state on disk.
- **Affected Files**: `internal/hub/adapters/ide/mandate.go`, `internal/knowledge/rule.go`, `internal/ast/rule.go`, `internal/memory/rule.go`.

## Test Cases & Acceptance Criteria

### Feature: Mandate preamble carries the resume and sync-delay clauses
Ref: UC-03, UC-04

Scenario: Resuming after interruption is in the preamble.
```gherkin
Given the mandate preamble rendered by mandatePreamble
When the text is inspected for the resume clause
Then it states that an interruption, a correction or a redirection re-applies the mandate in full
  And it says the framework tools keep precedence over native tools on resume
```

Scenario: The reindex delay is in the preamble.
```gherkin
Given the mandate preamble rendered by mandatePreamble
When the text is inspected for the indexing clause
Then it says automatic indexing lags the change
  And it names the sync tool as the way to be certain the indexes are current
```

Scenario: The preamble does not introduce pseudo-tags.
```gherkin
Given the mandate preamble rendered by mandatePreamble
When it is scanned for angle-bracket pseudo-tags
Then none are present, so parseTriggers cannot mistake prose for a trigger tag
```

### Feature: Knowledge module orders the task log before the work
Ref: UC-01, UC-02

Scenario: The mandate trigger covers the start of a task.
```gherkin
Given the doc_rule mandate trigger
When its trigger list is inspected
Then one entry fires when a task is about to start
  And one entry fires when a step finished or the direction changed
  And one entry fires when work is resumed after an interruption
```

#### Scenario: The skill orders creating the log before editing
```gherkin
Given the knowledge skill content
When the Task Logs section is inspected
Then it says the log is written before the work, not as a report at the end
  And the create-or-update table has a row for starting a task
  And the full template contains a Plan and Task Breakdown section
```

#### Scenario: The skill teaches how to resume
```gherkin
Given the knowledge skill content
When the resume guidance is inspected
Then it orders reading the existing task log first
  And it says a correction does not suspend the framework tool priority
```

### Feature: Every module states that indexing lags
Ref: UC-04

Scenario Outline: The delay is stated from within each skill
```gherkin
Given the "<skill>" rule content
When it is inspected for the indexing delay
Then it says the reindex lands after the write
  And it names the call that makes currency certain

Examples:
  | skill     |
  | knowledge |
  | ast       |
  | memory    |
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go` | Modified | Two global clauses in the preamble: resume and reindexing delay |
| `internal/knowledge/rule.go` | Modified | Mandate triggers added, plus sections for opening, continuous updates, and resuming in the skill |
| `internal/ast/rule.go` | Modified | Indexing delay noted next to the mandatory end-of-session sync |
| `internal/memory/rule.go` | Modified | Indexing delay noted, plus the choice between `memory_index` and a full sync |
| `internal/hub/adapters/ide/mandate_test.go` | Created | Ties together the two clauses of the preamble |
| `internal/knowledge/rule_test.go` | Created | Ties together opening-before-work, continuous updates, and resuming |
| `internal/ast/rule_sync_delay_test.go` | Created | Ties together the delay in the AST skill |
| `internal/memory/rule_sync_delay_test.go` | Created | Ties together the delay in the memory skill |
| `AGENTS.md`, `.claude/skills/**/SKILL.md`, `.agents/**`, `.kiro/**` | Regenerated | Artifacts generated by the builders above |

## Trade-offs & Decisions

The two new clauses live in the preamble, not repeated per module. The preamble exists exactly because five copies of the same policy would cost five times as much at the top of every session and teach nothing new by the fifth repetition. Resume and indexing lag are facts about the mandate as a whole — they are equally valuable for memory, graph, wiki, hub, and improvements. The operational detail stays in the skill: the mandate states that there is a delay and that sync is the fix; which narrow tool serves each case (`knowledge_sync`, `memory_index`, `ast_index`) is left to the skills, where the agent already is by the time it needs to choose.

"Call sync when you need certainty" and "don't call sync on every edit" would contradict each other if placed side by side without a criterion, so it is written this way: the trigger is not "I edited," it is "I am about to decide based on this" or "I am declaring this complete."

The `## Plan & Task Breakdown` section is part of the full template only, not the Quick one. The quick log exists for small changes; forcing it into a formal plan would turn minimal documentation into ceremony with no practical benefit.

## Technical Debt

The task log's opening is enforced by convention in text, not by validation. A log that stays `in-progress` forever is indistinguishable from an abandoned one — `knowledge_lint` could report `in-progress` logs older than N days.

The reindexing delay is described only qualitatively, as "a small delay." The actual value comes from the daemon watcher's debounce; nobody has measured it, and the skill makes no numeric promise.

## System Knowledge

`AGENTS.md` and the `SKILL.md` files are generated and versioned, but the daemon runs the installed binary (not the source tree), and regenerates them periodically whenever the runtime is newer than the source. Any regeneration done from source alone gets reverted within seconds. Editing the `.md` files by hand is wrong, and even editing the builder's Go source and regenerating from source is not enough without also updating the runtime — see the corresponding memory for the exact procedure (stop whichever runtime process holds the PID, rebuild with the Makefile's ldflags, replace it with `mv`, and touch `.build-id`/`launcher.stamp`).

The mandate preamble cannot contain `<word>` pseudo-tags. `parseTriggers` scans the block's content for `<word>` and treats any such match as a module tag; a placeholder left in prose would turn into a phantom trigger that `assembleTriggers` would then re-emit.

Editing the builder after installing it makes the installed binary stale relative to source. Installing runs the launcher, which re-extracts `graphit-core`/`graphit-mcp` when its BuildID no longer matches `.build-id`/`launcher.stamp` — but any subsequent edit to `rule.go` falls behind the installed version again, and the builder must be rebuilt and reinstalled to regenerate the installed text. Order of operations: edit → test → `make install` → `graphit sync` → review the diff.

`MandateTrigger` receives `alwaysClause` as the last paragraph in its block. It is the only place an unconditional statement can fit; the list of triggers is conditional by construction.

## Progress Log

### 2026-08-19

Memory and wiki were consulted before any edit; the graph located the five `MandateTrigger` call sites and `mandatePreamble`, and memory surfaced the binary-installation trap (T7) before it could bite.

Task log created before implementation, with the plan already broken into the seven items this task introduces.

T1–T5 implemented across the four builders. T6: eight new tests across four files, all green; packages `internal/hub/...`, `internal/knowledge`, `internal/memory`, `internal/ast`, `cmd/graphit/commands`, and `internal/livesearch/...` green with `-tags fts5`.

Mid-T7 course correction recorded because it cost a second build round: while reviewing the generated `SKILL.md`, I noticed the note about relative paths had ended up after two new sections instead of next to the table it belongs to, so I reordered it — and that invalidated the binary I had just installed, so I had to regenerate and reinstall a second time. The correct order, now recorded: edit builder → test → `make install` → `graphit sync` → check the diff of the `.md` files.

Result: `AGENTS.md` and nine `SKILL.md` files (AST, Knowledge, Memory × `.claude/skills`, `.agents`, `.kiro`) were regenerated with the new text, and the daemon restarted on the freshly re-extracted runtime — i.e., it no longer reverts.

Blocker found: the MCP tool `graphit_sync` went 1800s without responding and was aborted by the client, while `graphit sync` via the CLI completed. The MCP stdio process for this session was running the old binary (started before `make install`), and the call happened right in the window where the daemon was restarting — so this is not a conclusion about the tool in general, but whoever repeats the "install and sync in the same session" sequence should expect it.
