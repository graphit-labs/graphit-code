---
title: Skills are read just-in-time, not all at session start
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [mandate, skills, context-budget, ide-rules]
---

# Skills are read just-in-time, not all at session start

## Objective

The Engineer reported that agents open **every** module skill at the beginning of a session,
before any of those domains is actually involved in the work — "garanta que as skills só sejam
lidas no momento que tiver necessidade de usá-las, para que não leia tudo no começo sem
precisar". The mandate must guarantee lazy, per-need skill loading.

Why this happens today, read from the generated mandate text itself:

1. `mandatePreamble()` opens with an unconditional order — *"Whenever you are about to perform
   any action, you MUST first read and use the corresponding skill. Always read the
   corresponding skill before proceeding."* Read literally, "any action" includes the first
   turn of the session, and "always" reads as "in advance, to be safe".
2. The MCP-FIRST section then says *"If you are unsure whether one applies, it applies"*. That
   line exists to stop an agent from talking itself out of a trigger that did fire, but with
   nothing scoping it to the action currently in hand it also licenses preloading all four
   skills, since one cannot be *sure* a domain will not come up later.
3. Nothing in the preamble ever states the opposite obligation: that a skill whose trigger has
   not fired must be left unread. Every module block pushes towards reading; no line pushes
   back.

The cost is concrete: the four skills are large, and a session that only edits one Go file pays
for the AST, knowledge, hub and memory skills in full before the first tool call — context that
is then unavailable for the work.

## Reasoning and approach justification

The rule being added is a **policy identical for all five modules**, so per the recorded
decision `[[politica-que-vale-para-os-cinco-modulos-vai-no-mandatepreamble-nao-repetida-em-cada-mandatetrigger]]`
it belongs in `mandatePreamble()` (`internal/hub/adapters/ide/mandate.go`) and must NOT be
repeated inside each `MandateTrigger()`. Repeating it per module would cost five copies of the
same paragraph at the top of every session — the exact waste this task is about.

Alternatives considered and dropped:

- **Shrink the skills instead.** Rejected: it treats the symptom. A smaller skill read
  needlessly is still a skill read needlessly, and the trigger lists already exist precisely to
  decide *whether* to read.
- **Drop the "if unsure, it applies" line.** Rejected: it is load-bearing — without it agents
  reclassify a structural question as "just a quick grep". The fix is to scope it to the action
  in hand, not to remove it.
- **Add the rule to each module block.** Rejected by the recorded decision above.
- **Also remove memory's session-start trigger.** Rejected: searching memory before the first
  response is a framework invariant, not an eager read of convenience. Instead the trigger is
  marked as the single exception, which makes the contrast explicit rather than leaving the
  reader to infer it.

## Plan and Task Breakdown

- [x] **T1 — Add a lazy-loading section to `mandatePreamble()`** — Spec: touches
  `mandatePreamble()` in `internal/hub/adapters/ide/mandate.go`. Done when the preamble states
  that a skill is opened at the moment an action in its domain is about to happen, that a skill
  whose trigger has not fired must stay unread, and that a skill already read this session is
  not re-read. Constraint: the preamble text MUST NOT contain any `<word>` pseudo-tag —
  `parseTriggers` scans `<(\w+)>` and would reassemble the prose as a phantom trigger block
  (`TestMandatePreambleHasNoPseudoTags`).
- [x] **T2 — Rewrite the opening line and scope "if unsure, it applies"** — Spec: same
  function. Done when the opening order is conditional on the action being in a module's domain
  and the unsure-clause is bound to the action currently in hand rather than to the session.
  Constraint: `ide_test.go` asserts the preamble still contains `ABSOLUTE PRECEDENCE` and the
  other invariant-policy phrases; `mandate_resume_test.go` asserts the interruption and
  indexing-lag sections survive.
- [x] **T3 — Mark memory's session-start trigger as the one exception** — Spec: touches
  `MandateTrigger()` in `internal/memory/rule.go`, first trigger entry only. Done when the
  trigger states that this is the only skill due before the first response. Constraint: the
  trigger list is rendered verbatim into AGENTS.md, so no pseudo-tags here either.
- [x] **T4 — Cover the new policy with a test** — Spec: adds a test beside the existing
  preamble tests in `internal/hub/adapters/ide/`. Done when the test fails if the lazy-loading
  policy is dropped from the preamble.
- [x] **T5 — Verify** — Spec: `go build ./...` plus the `internal/hub/adapters/ide`,
  `internal/memory`, `internal/knowledge` and `internal/ast` test packages.
- [x] **T6 — Regenerate the tracked artifacts** — Spec: `AGENTS.md`, `CLAUDE.md` and the
  versioned `SKILL.md` files are generated by the **installed** binary, not the source tree. Per
  `[[o-daemon-reescreve-agents-md-e-os-skill-md-versionados-a-partir-do-binario-instalado-nao-do-source-tree]]`
  the order is: edit builder → test → `make install` → `graphit sync` → check the diff. Skipping
  the install means the daemon reverts the generated files within seconds.

## Implementation Details

### `internal/hub/adapters/ide/mandate.go` — `mandatePreamble()`

Three changes, all inside the shared preamble:

1. **Opening line** — was *"Whenever you are about to perform any action, you MUST first read
   and use the corresponding skill. Always read the corresponding skill before proceeding."*
   Now conditional on the action falling in a module's domain, and explicit that the skill to
   read is *that* module's, at *that* moment.
2. **New section `## ONE SKILL, AT THE MOMENT YOU NEED IT — NEVER ALL OF THEM UP FRONT`** —
   states the lazy rule and the three corollaries agents get wrong: no trigger fired means the
   skill stays closed; a skill already read this session is not re-read (including after an
   interruption, where what re-applies is the tool priority, not the file read); and a plausible
   future need is not a trigger.
3. **The unsure-clause** in MCP-FIRST is now scoped to *the action you are about to take* and
   says what it is not — a licence to preload.

### `internal/memory/rule.go` — `MandateTrigger()`

The session-start trigger now names itself as the single exception, so the contrast with the
lazy rule is stated rather than inferred.

## Use Cases

### UC-01: Session starts with a request that touches one domain only
- **Actor**: The agent, on its first turn.
- **Preconditions**: `AGENTS.md` carries the regenerated mandate block; the user's request
  matches the triggers of exactly one module.
- **Main Flow**:
  1. The agent reads the mandate block from `AGENTS.md`.
  2. It runs the session-start memory search — an MCP call, `graphit_memory_search`.
  3. It evaluates each module's trigger list against the request.
  4. It opens ONLY the skill whose trigger fired, at the moment it is about to act in that
     domain.
- **Alternative Flows**:
  - A second domain becomes involved mid-session: that module's skill is opened then, not
    retroactively at the start.
- **Error Scenarios**:
  - Trigger evaluation is ambiguous for the action in hand: the unsure-clause applies and the
    skill is opened. Ambiguity about a *later* action is not a trigger.
- **Postconditions**: Context holds one skill instead of four; the skills for untouched domains
  were never loaded.
- **Affected Files**: `internal/hub/adapters/ide/mandate.go`, `internal/memory/rule.go`

### UC-02: The agent is interrupted or corrected mid-task
- **Actor**: The agent, resuming.
- **Preconditions**: A skill was already read earlier in the same session.
- **Main Flow**:
  1. The agent re-applies the MCP-tool priority, as `AN INTERRUPTION IS NOT AN EXEMPTION`
     requires.
  2. It re-consults the skill content already in context, without re-reading the file.
  3. It re-runs the lookups (memory, wiki, graph) that the correction invalidated.
- **Alternative Flows**:
  - The resumed work enters a domain whose skill was never read: that skill is opened now.
- **Error Scenarios**:
  - Re-reading an already-loaded skill: wasted context, and the IDE's activation tool refuses
    it as already active.
- **Postconditions**: Tool priority restored without paying for the skills a second time.
- **Affected Files**: `internal/hub/adapters/ide/mandate.go`

## Test Cases & Acceptance Criteria

### Feature: Lazy skill loading stated in the shared mandate preamble
Ref: UC-01, UC-02

#### Scenario: The preamble forbids preloading every skill
```gherkin
Given the mandate preamble generated by the running binary
When the preamble text is inspected
Then it contains the section heading "ONE SKILL, AT THE MOMENT YOU NEED IT"
  And it states that a skill whose trigger has not fired stays unread
  And it states that a skill already read in the session is not read again
```

#### Scenario: The preamble carries no pseudo-tag
```gherkin
Given the mandate preamble generated by the running binary
When the text is matched against the pattern for an angle-bracket word
Then no match is found
  And parseTriggers therefore cannot read the prose as a trigger block
```

#### Scenario: The invariant policy the preamble already owned survives
```gherkin
Given the mandate preamble generated by the running binary
When the preamble text is inspected
Then it still contains "ABSOLUTE PRECEDENCE"
  And it still contains "AN INTERRUPTION IS NOT AN EXEMPTION"
  And it still contains "AUTOMATIC INDEXING LAGS THE CHANGE"
```

#### Scenario: Memory's session-start trigger is marked as the single exception
```gherkin
Given the memory module mandate trigger block
When its first trigger entry is inspected
Then it states that memory is the only skill due before the first response
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go` | Modified | Lazy-loading section, conditional opening line, scoped unsure-clause |
| `internal/memory/rule.go` | Modified | Session-start trigger marked as the one exception |
| `internal/knowledge/rule.go` | Modified | Resume trigger said "re-open this skill", which contradicts the new rule; now says re-apply the protocol |
| `internal/hub/adapters/ide/mandate_lazy_skill_test.go` | Created | Locks the lazy-loading policy into the preamble |
| `AGENTS.md` | Regenerated | Generated artifact, rewritten from the installed binary by sync. `CLAUDE.md` is only an `@AGENTS.md` pointer and did not change |

> The `SKILL.md` files under `.agents/`, `.claude/`, `.codex/`, `.kiro/` and `.opencode/` also
> appear modified after the sync. Those additions come from **other** uncommitted source edits
> that were already in the working tree before this task; the mandate change touches trigger
> and preamble text, which lives in `AGENTS.md` only.

## Trade-offs & Decisions

- **Preamble over per-module blocks.** The rule is module-invariant, so it goes in the preamble
  once. Cheaper at session start, which is the point of the task.
- **Kept "if unsure, it applies" instead of deleting it.** Scoping it to the action in hand
  preserves the behaviour it was added for (agents talking themselves out of a fired trigger)
  while removing the reading it was being given (preload everything just in case).
- **Kept memory's session-start read.** One eager skill, stated as the exception, beats an
  invariant that agents then reconstruct wrongly. The alternative — deferring the memory skill
  and keeping only the raw search obligation — was rejected because the search protocol
  (search answers with titles, read with `wiki_source`) lives in the skill, and an agent that
  searches without it acts on titles it never opened.

## Technical Debt

- [x] The knowledge module's resume trigger read *"re-open this skill instead of continuing from
  what you remember"*, which contradicts the new rule outright. Fixed in the same task: it now
  says re-apply the protocol, and states that an already-read skill has nothing to open again.
- [ ] The skill bodies themselves still open with unconditional "ALWAYS consult this skill"
  framing in their own text. That is harmless once a skill is already open — you only read it
  after a trigger fired — but a future pass could align the wording so a skill does not appear
  to claim it should have been read earlier than it was.

## System Knowledge

- The mandate inner content is parsed by `parseTriggers`, which treats `<(\w+)>` as a trigger
  tag, so **no prose in the preamble or in any trigger entry may contain an angle-bracket
  word**. `TestMandatePreambleHasNoPseudoTags` guards the preamble specifically.
- `mandatePreambleHash()` is recorded in the project's mandate hash cache, so a preamble-only
  change does propagate to already-installed projects — the fast path compares the preamble
  hash separately from the trigger hash (`mandate_preamble_propagation_test.go`).
- `alwaysClause` is the last paragraph of a module block and the only place in a trigger where
  an unconditional statement belongs; the trigger list itself is conditional by construction.

## Progress Log

### 2026-09-01
- Searched memory before touching anything; the decision on preamble-vs-trigger placement and
  the correction about the daemon regenerating `AGENTS.md` from the installed binary both
  applied directly and shaped the plan.
- Read `mandatePreamble()`, `ModuleMandateTrigger()` and `internal/memory/rule.go`'s
  `MandateTrigger()` through the AST source tool; identified the three lines that produce the
  eager-read behaviour.
- Wrote this log with the plan before the first edit.
- T1/T2 landed: `mandatePreamble()` now opens conditionally, carries the new
  `ONE SKILL, AT THE MOMENT YOU NEED IT` section, and the unsure-clause is scoped to the action
  in hand.
- T3 landed: memory's session-start trigger names itself the single exception.
- T4 landed: `mandate_lazy_skill_test.go` asserts the section heading, the three corollaries and
  the absence of pseudo-tags in the new text.
- T5: `go build ./...` clean; `internal/hub/adapters/ide`, `internal/memory`,
  `internal/knowledge` and `internal/ast` test packages pass.
- Found and fixed a direct contradiction while re-reading the generated output: knowledge's
  resume trigger ordered the agent to *re-open* the skill. Changed to re-apply the protocol.
- T6, and a trap worth recording: `make install` alone did NOT restart the daemon. The
  installed launcher only re-extracts the runtime **when it is invoked**, so the daemon kept
  running the old core (uptime unchanged) and a sync would have regenerated the OLD text. One
  launcher invocation (`graphit --version`) re-extracted `~/.graphit/runtime/dev/`, the
  `launcher.stamp` changed, and the daemon restarted ~60s later on the new build. Only then did
  sync regenerate `AGENTS.md` with the new preamble. Also note the builder was edited a second
  time after the first install, so `make install` had to run twice — the recorded order
  (edit → test → install → sync) is per edit round, not per task.
- Verified the regenerated `AGENTS.md` diff: new opening line, the lazy-loading section with its
  blank line before `## MCP-FIRST`, the scoped unsure-clause, and both trigger edits.
