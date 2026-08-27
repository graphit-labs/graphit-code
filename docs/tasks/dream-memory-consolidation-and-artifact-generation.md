---
title: "Dream consolidates memory for real, and can create artifacts again"
description: "Rewrote the dream module's memory consolidation as an analyse/apply split with invariants enforced in Go, unblocked agentic artifact generation, fixed the permanent deep sleep, and removed the destructive memory maintenance from sync."
content-type: task-log
audience: developers
keywords:
  - dream
  - memory
  - consolidation
  - deep sleep
  - agentic
related:
  - "docs/specs/dream_module.md"
  - "docs/specs/improvements_module.md"
  - "docs/guides/cli_reference.md"
  - "docs/guides/mcp_tools_reference.md"
---

# Dream consolidates memory for real, and can create artifacts again

**Date:** 2026-08-13

## Objective

Make the dream module do the two things it was specified to do and did not:
consolidate the memory store — finding duplicates and contradictions and *resolving*
them — and create the artifacts it plans. Remove the impediments rather than
document around them. Retire the memory maintenance that ran inside `sync`, and move
opportunistic sanitisation into the agent's own instructions.

## What was wrong

An audit of the module found the following, all latent in this repository because
`modules.dream` is not set here.

### The agent could not create anything

`ai.cliClient.completeInternal` prepends `nonInteractivePreamble` to every prompt,
which says *"Do NOT execute actions that require user approval (file edits, shell
commands, etc.)"* and *"Respond directly with your analysis… as plain text"*. Every
deliverable of a dream session is a file edit: skills, memories, the deep sleep
sentinel, the backlog result file, the report. The prompt and the transport
contradicted each other, and the prohibition came first.

Cascade: no `.exhausted` sentinel meant `checkDeepSleep` never fired, so `tick`
redispatched every `checkInterval` (10 minutes) for as long as the project stayed
idle — roughly six sessions an hour, three LLM calls each, indefinitely. The backlog
item was never marked done, so the same item was reanalysed every cycle.

### Deep sleep was permanent, and the session never rotated

`tick` assigned `state.LastUserModTime = lastMod` and then called
`resolveSessionULID(lastMod)`, which decided rotation with
`currentModTime.After(r.state.LastUserModTime)` — mtime against itself, always
false. So `needsNew` was true only when `CurrentULID` was empty. Every later session
reused one id and overwrote one report file, and since `Exhausted` is cleared only
inside that branch, the first deep sleep lasted forever. The specification claimed
the opposite.

### Consolidation analysed and then discarded the analysis

`runMemoryConsolidation` collected `Contradictions`, `Stale` and `Suggestions` into
one slice and applied `if action.Type == "delete"`. Contradictions carry
`"conflict"`, stale carries `"update"`, and promote/demote/update suggestions carry
their own names — so **only** an explicit `DELETE` suggestion did anything. The
model was asked which memory was outdated and which to keep, and the answer was
dropped. `report.AIAnalysis` was written and never read in production. The log line
said `applied — N total actions` with `N` counting proposals, including the ones
that did nothing.

Where it did act, it was destructive: duplicate merges wrote
`filepath.Join(dir, ID0+".md")` with `type: consolidated` frontmatter, so an
important survivor (stored as `<id>_important_.md`) gained a second file instead of
being merged; the original type was replaced; and because the parser never filled
`NewContent`, `mergedContent` fell back to `action.Reason` — the model's
justification line written in place of both bodies. All of it bypassed
`MemoryService`, so git and the wiki never learned.

### Updating a memory silently declassified it

Not part of the original audit, found while wiring the fix. `UpdateMemory` rebuilt
frontmatter from id, title, scope, scope_id, created_at, updated_at and a hardcoded
`tags: [memory]`. Type, importance, `project_id` and every user tag were dropped. An
important `correction` tagged `auth,security` came back untyped and untagged, with a
valid file and the right body, and nothing reported it.

### Two more found by the new tests

`ParseMemoryFrontmatter` never read `updated_at`, and `changeRelevance` renamed the
file without touching the frontmatter flag — so a promoted memory carried
`important:` absent while its filename said otherwise, and anything trusting the
frontmatter read the stale value.

### `sync` deleted memories

`runMemoryMaintenance` ran `RunGC(scope, 90)` and then `os.Remove` straight into the
worktree. Destructive work with no report, no confirmation and no undo, triggered by
a command developers run to *index* things, bypassing `MemoryService` entirely.

## What changed

### Consolidation: analyse, then apply

Split in two. `RunConsolidation` returns a plan and writes nothing;
`ApplyConsolidation` executes it through `MemoryService`. The invariants are in Go,
not in the prompt — see the [specification](../specs/dream_module.md) for the list.
Briefly: content is never dropped, importance and classification are never lost, an
important memory is never deleted by a suggestion, the store is never emptied, the
survivor is written before the others are removed, and every refusal is reported.

The analysis now asks for JSON, with the sectioned-text parser kept as a fallback.
`KeepID` is a separate field: the old text parser flattened every bracketed id on a
`CONFLICT` line into one list, including the one the analysis named as the survivor.
The corpus is batched by character budget, so a store larger than one context window
is analysed rather than truncated. Every returned id is validated against the real
corpus.

`AIFailed` distinguishes "the analysis never happened" from "nothing to do"; the
error was previously swallowed into a string nobody read.

### The agent runs agentically

`executeLocal` now uses `ai.StreamClient.CompleteStream` with `AllowTools: true` and
`WorkDir` set to the project — the same path live search already used, rather than a
second mechanism. Two additions to `internal/ai`: `agentArgs`, resolved from
`ai.agent_args` / `ai.agent_args.<binary>` and passed only to tool-allowed runs, and
nothing else. The permission flag is operator-configured because it differs per CLI
and changes between releases.

When the client cannot run agentically, or the agent finishes without using a single
tool, the runner logs a warning saying artifacts were probably not created. It no
longer looks like success.

### Report ownership

The runner fingerprints the report path around the agent run. If the agent wrote it,
that version is kept and the consolidation audit is appended; otherwise the runner
writes the wrapper. A failed agent still produces a report recording the
consolidation that already happened.

### Runner state

`SessionModWatermark` is now separate from `LastUserModTime`, so rotation compares
against a field `tick` does not overwrite. New activity clears `Exhausted`, which is
what the specification always claimed.

### Removals

- `graphit memory consolidate` (command and runner) — deleted.
- The `RunGC` + `os.Remove` block inside `runMemoryMaintenance` — deleted. Wiki
  regeneration stays.
- `graphit_memory_consolidate` from `mcp_tools_reference.md` — the tool never existed
  in the server.

`graphit memory gc` stays for the narrow question it answers well. Its MCP `dry_run`
is now a `*bool`, so absent means dry run; a bare call used to delete everything it
found. Its staleness threshold is one constant shared with consolidation.

### Skill

`internal/memory/rule.go` gained **Sanitise On Sight**: a table of what to do when
an agent notices duplicates, contradictions, deprecated or vague memories, with the
ordering rule that makes deletion safe (carry the content forward, then delete) and
an explanation of why this is the agent's job rather than a scheduled pass's — the
agent has the task context that makes the judgement possible. The contradiction
protocol now prefers `update` over delete-then-insert, and the mandate lists the new
trigger.

## Trade-offs & Decisions

- **Deterministic apply, model-driven analysis.** The alternative was to let the
  dream agent perform the consolidation with its own tools. Rejected: whether the
  agent can write at all depends on which CLI is installed and how it is configured,
  and "the memories were consolidated" must not be contingent on that.
- **`ai.agent_args` is empty by default.** A built-in per-CLI flag table would be
  guesswork about external tools, and the failure modes are an unparseable flag or an
  over-permissioned agent. The cost is that operators must set it; the runner warns
  when the result looks like a session that could not write.
- **90 days for staleness, not 30.** Consolidation and GC now share one threshold. At
  30 days nearly every memory in this repository would be flagged every cycle, and a
  report full of "verify this" is a report nobody reads.
- **`memory gc` kept.** It answers a different question from consolidation — empty
  or unclassified and old — and it is the only path that deletes by age. Consolidation
  refuses to.
- **JSON is the only accepted analysis format.** The first pass kept the sectioned
  text parser as a fallback; it was then deleted, along with ~130 lines of parsing and
  23 tests. A second, looser parser buys malformed answers that still parse, at the
  price of partially-understood plans applied to real memories — and it makes an
  unusable analysis indistinguishable from a clean corpus. One contract, and an error
  that says so.
- **Report location unchanged by default.** `dream.reports_dir` now exists, as the
  specification already claimed, but the default stays in the brand directory: the
  vault holds generated output, a daemon sentinel, and a per-developer read marker
  that would conflict on every read if versioned.

## Files changed

| File | Change |
|---|---|
| `internal/memory/consolidate.go` | Rewritten: plan-only analysis, JSON + fallback parsing, batching, `KeepID`, `AIFailed` |
| `internal/memory/consolidate_apply.go` | New: `ApplyConsolidation`, invariants, `ConsolidationOutcome` and its report |
| `internal/memory/gc.go` | New: GC moved out of `consolidate.go`, threshold shared, `ApplyGC` goes through the service |
| `internal/memory/memory.go` | `MemoryFrontmatter`, `ParseMemoryFrontmatter`, `renderMemoryFile`, `updatedMemoryContent`, `UpdateMemoryTyped`, `withImportantFlag`; `buildMemoryFile` delegates |
| `internal/memory/rule.go` | Sanitise On Sight; contradiction protocol prefers update; gc section corrected; new mandate trigger |
| `internal/dream/dream.go` | Watermark, wake from deep sleep, consolidation via the new API, report ownership, agentic execution, honest logging |
| `internal/dream/prompt.go` | Consolidation briefing; single report instruction; memory guidance via MCP tools |
| `internal/dream/reports.go` | Honours `dream.reports_dir` |
| `internal/ai/cli.go`, `cli_stream.go`, `ai.go` | `agentArgs` from config, applied to tool-allowed runs |
| `internal/config/config.go` | `ResolveDreamReportsDir` |
| `internal/mcpstdio/tools_memory.go` | `dry_run` defaults to true; `gcDryRun`; description scoped |
| `cmd/graphit/commands/{memory,runners,lifecycle}.go` | `consolidate` removed; GC removed from sync |
| `docs/specs/dream_module.md` | Consolidation, invariants, state fields, report ownership, config keys |
| `docs/guides/{cli_reference,mcp_tools_reference}.md` | Removed command and non-existent tool; gc documented accurately |

## Tests

New: `internal/memory/consolidate_apply_test.go` (merge folds and preserves
importance and type; union built when no content supplied; contradiction keeps the
recommended memory; refuses to delete an important memory or the last one; stale
without content is reported not applied; actions on removed memories skip; a failed
survivor write removes nothing; promote/demote; outcome reports refusals),
`internal/memory/memory_update_test.go` (classification survives an update, body
survives a rename, reclassification moves the type tag, invalid type ignored, create
and update agree), `internal/dream/report_ownership_test.go` (fingerprint contract,
audit appended, consolidation survives a failed agent, briefing carries refusals),
`internal/mcpstdio/tools_memory_gc_test.go` (dry run is the default).

Updated: the stale fixtures for the 90-day threshold; `TestRunnerTickExhausted` now
sets the watermark, because the old fixture described a state the runner cannot
reach; `rule_gc_test.go` inverted along with the default it pins.

Two of the new tests failed first and found real bugs — the unparsed `updated_at`
and the frontmatter flag left behind by promotion.

```
go test -tags fts5 ./internal/memory/ ./internal/dream/ ./internal/ai/ \
  ./internal/config/ ./internal/mcpstdio/ ./internal/uiserver/ \
  ./internal/daemon/ ./internal/backlog/ ./cmd/graphit/commands/
```

All pass.

## Second pass — nothing is kept for compatibility

The project is pre-release, so the two concessions above were removed rather than
carried.

### The session id is no longer called a ULID

`CurrentULID` → `CurrentSessionID`, the JSON tag `current_ulid` →
`current_session_id`, `resolveSessionULID` → `resolveSessionID`, every `ulid`
parameter → `sessionID`, and the specification with them. The external contract was
already correct — the HTTP and MCP responses always said `SessionID` — so this was
internal only. A `dream.state` written by an older build now loses its session id and
the next tick opens a new session, which is the right trade while the format moves.

### The sectioned text parser is gone

`parseConsolidationText`, `parseGroupSection`, `parseSuggestionSection`,
`parseConsolidationSection`, `extractBracketedIDs`, `extractKeepID`, `dropID`,
`sectionBody` and `parseConsolidationType` — deleted, with the 23 tests that covered
them. `aiConsolidation` now errors on a non-JSON answer, which surfaces as `AIFailed`
in the report.

### Two things found while doing it

**A GC rule existed only in the tests and the help text.** `graphit memory gc --help`
claimed it collected memories "Very old (>2× threshold) regardless of type", and
`TestRunGC_VeryOldClassified` asserted exactly that. Production never implemented it.
The test passed because it ran against a copy of the GC loop that lived in the test
file and *did* have an `age > 2*threshold` branch. Documentation and test agreed with
each other and both disagreed with the code.

The code was right: deleting a classified memory on age alone contradicts the rest of
this design, and an unmarked `correction` at 200 days is precisely the memory that
stops a repeated mistake next quarter. The claim is gone from the help text, and the
test now asserts the opposite — that a classified memory is never a candidate by age.

**Four test helpers reimplemented production.** `runConsolidationInDir`,
`consolidationHelper`, `runGCInDir` and `runGCInDirBoost` each carried their own copy
of the snapshot-loading or GC loop, because `RunConsolidation` and `RunGC` only
accepted a scope. Tests asserting against a private reimplementation assert nothing
about production — which is how the 2× rule survived. Both functions now delegate to
`consolidateDir` and `gcDir`, the helpers are deleted, and the tests drive real code.

## Third pass — GC removed, consolidation reachable without the dream

Two changes, both narrowing the surface rather than widening it.

### `memory gc` is gone, not relocated

The CLI command, the `graphit_memory_gc` MCP tool, `internal/memory/gc.go`
(`RunGC`, `ApplyGC`, `GCReport`, `GCCandidate`) and 13 tests were deleted.

Collecting memories by age answers the wrong question. Age says a memory has not been
revised, not that it is wrong, and the memories that sit unread for months are exactly
the conventions and corrections that later stop a repeated mistake. Every guard the
tool accumulated — never important, never classified, dry run by default — was working
around the fact that its central signal did not mean what deletion requires.

Consolidation reasons about content instead, and it removes a memory only after
carrying that content into a surviving one. That is the mechanism that should exist,
so it is now the only one.

### Consolidation no longer depends on the dream module

`graphit memory consolidate` runs the same engine on demand: analysis on the agent CLI
from `ai.cli`, application in Go under the invariants. Dry run by default; `--user` for
user scope. Without an AI CLI it degrades to the deterministic staleness check and says
so.

The output is the plan, then what was applied, refused and failed — refusals included
deliberately, since each one is a judgement the invariants declined to make
unattended.

**There is no consolidation MCP tool, and that is the design.** An agent reading
memories already holds what makes each judgement correct: which memory matched the
task, which misled it, which the code has outgrown. It resolves duplicates and
contradictions with `memory_update` and `memory_delete` as it encounters them — the
Sanitise On Sight protocol in the skill. Giving it a batch tool would let it trade that
context for a background pass's caution. So the whole-store pass has exactly two entry
points, both outside MCP: the dream module on idle, and the CLI for a developer at a
terminal.

The memory skill mentions neither the command nor a tool: the agent's instruction is to
fix what it sees, and a command it should not shell out to is noise in its context.

### Help text corrections found along the way

- `memory delete <slug>` → `<id>`. Files are named by ID, so the documented slug
  examples could not have worked.
- `memory search` claimed a "grep-like search over all memory files". It calls
  `wiki.BM25Search` over the compiled wiki — same engine as the MCP tool. The
  consequence is the opposite of what the text implied: a memory written moments ago
  may not appear yet.
- Four flags were declared, documented, and never read: `--context` on `index`,
  `insert` and `delete`, and `--louvain` on `index`. `memory delete X --context foo`
  silently deleted from the project scope. Removed.

### Still open from this pass

- **`memory watch` watches the wrong thing.** `runMemoryWatch(scope, ...)` passes
  `"project"`/`"user"` into a parameter named `rootPath`, which becomes
  `fswatch.Config{Root: ...}` — a relative path that does not exist. It also calls
  `RunProjectCycle` unconditionally, so `--user` cannot work either. Not fixed here:
  it needs a decision about which directory it should watch (the raw worktree store,
  presumably) rather than a text change.

## Still open

- **`ai.agent_args` needs a value per environment** before a dream session can write
  anything. The warning tells an operator when this is the problem, but nothing
  discovers the right flag for them.
- **Cross-batch duplicates.** A corpus split across analysis batches will not have
  duplicates detected between batches.
