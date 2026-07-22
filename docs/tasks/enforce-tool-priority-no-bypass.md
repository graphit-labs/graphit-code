---
title: Enforce Graphit MCP Tool Priority & No-Bypass Across All 5 Module Rules/Skills
status: done
created: 2026-07-21
updated: 2026-07-21
tags: [rules, skills, mandate, mcp, governance]
---

# Enforce Graphit MCP Tool Priority & No-Bypass Across All 5 Module Rules/Skills

## Objective

Guarantee that agents ALWAYS use the graphit MCP tools first — before their own
native/built-in tools — and never bypass them, across all five modules (memory,
ast, hub, knowledge, improvements). Enforcement must live in BOTH layers: the
`MandateTrigger()` blocks injected into `AGENTS.md` and the full skill content
(`RuleContent`). Every agent-facing reference must use MCP tools, never the CLI.
The mandate must unconditionally require the memory and AST skills.

## Implementation Details

### 1. Shared mandate-priority builder
- Added `ModuleMandateTrigger(heading, skillName, domain, alwaysClause)` in
  `internal/hub/adapters/ide/mandate.go`. It emits a standardized block asserting
  MCP-first priority, forbidding CLI usage, requiring the skill be read before
  acting, and (when `alwaysClause` is set) an unconditional "always use this skill"
  directive. The text is deliberately free of angle-bracket pseudo-tags so
  `parseTriggers` (which treats any `<word>` as a tag) recovers it unchanged.

### 2. Strengthened all 5 `MandateTrigger()` blocks
- `internal/memory/rule.go`, `internal/ast/rule.go`, `internal/hub/rule.go`,
  `internal/knowledge/rule.go`, `internal/improvements/rule.go` now call the shared
  builder. `mem_rule` and `ast_rule` carry the unconditional "ALWAYS consult this
  skill" clause (memory at session start + before acting; AST before any code
  exploration). Canonical trigger order (mem→ast→hub→doc→imp) is preserved.

### 3. Skill content — tool priority & no-bypass
- **memory** (`RuleContent`): new "Memory MCP Tools REPLACE Your Native Recall"
  section — why-it-replaces table, MUST-use table, "there is NO fallback" rule,
  anti-patterns, MCP-only.
- **ast** (`ASTRuleContent`): added an explicit "access via MCP only, never CLI"
  note reinforcing the already-strong AST-first section.
- **hub** (`HubRuleContent`): new "The Hub REPLACES Guessing and Your Built-in
  Knowledge" section — why-it-replaces, MUST/MUST-NOT tables, strict fallback
  gating, anti-patterns, MCP-only.
- **knowledge** (`KnowledgeRuleContent`): added the "MCP only, never CLI"
  reinforcement to the existing "Wiki-First — Replaces Your Tools" section.

### 4. New MCP tool: `graphit_hub_type-path`
- Added in `internal/mcpstdio/tools_hub.go`, wrapping
  `ide.ArtifactTypePath(projectDir, ide, type, name)` (mirrors the
  `hub type-path` CLI). This closes the CLI/MCP parity gap so the improvements
  skill can reference an MCP tool instead of the CLI.

### 5. CLI → MCP conversion (agent-facing content)
- `internal/improvements/rules.go` (`DefaultRules`): all `graphit memory ...`,
  `graphit hub type-path`, `graphit hub submit` invocations and shell `cat >`
  artifact creation converted to `graphit_memory_*`, `graphit_hub_type-path`,
  `graphit_hub_submit` MCP tools + IDE file tools.
- `internal/dream/prompt.go`: the `hub type-path` CLI reference converted to the
  `graphit_hub_type-path` MCP tool.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go` | Added `ModuleMandateTrigger` builder | Shared, consistent, parser-safe mandate blocks |
| `internal/memory/rule.go` | Rewrote `MandateTrigger`; added priority section | MCP-first + no-bypass, always-use guarantee |
| `internal/ast/rule.go` | Rewrote `MandateTrigger`; added MCP-only note | Consistency + always-use guarantee |
| `internal/hub/rule.go` | Rewrote `MandateTrigger`; added enforcement section | Hub-first over model/web knowledge |
| `internal/knowledge/rule.go` | Rewrote `MandateTrigger`; MCP-only note | Consistency |
| `internal/improvements/rule.go` | Rewrote `MandateTrigger` | MCP-first framing |
| `internal/improvements/rules.go` | Converted all CLI refs to MCP | MCP-only enforcement |
| `internal/dream/prompt.go` | Converted `hub type-path` CLI to MCP | MCP-only enforcement |
| `internal/mcpstdio/tools_hub.go` | Added `graphit_hub_type-path` tool | CLI/MCP parity |
| `internal/hub/adapters/ide/ide_test.go` | Test for `ModuleMandateTrigger` | Parser-safety + markers |
| `internal/mcpstdio/tools_test.go` | In-memory test for the new tool | Registration + behavior |
| `internal/improvements/improvements_test.go` | No-CLI / MCP-present regression test | Prevent CLI regressions |

## Trade-offs & Decisions

- Chose a shared builder only for the mandate block (genuinely identical across
  modules) but wrote the skill sections inline per module, since each references a
  different tool set — a single mega-helper would have been over-parameterized.
- Resolved the `graphit hub type-path` gap by adding the MCP tool (option C) per
  the requirement that every referenced command must have an MCP equivalent.
- Kept intentional CLI mentions inside anti-pattern tables (e.g. "Using the CLI
  ... instead of MCP tools") because they instruct the agent NOT to use the CLI.

## Test Cases & Acceptance Criteria

### Feature: MCP-first mandate + skills
Ref: this task

#### Scenario: mandate asserts MCP priority and no-bypass
```gherkin
Given the regenerated AGENTS.md
When I inspect each of the 5 <*_rule> blocks
Then each contains "MCP-FIRST", "ABSOLUTE PRECEDENCE", "NEVER via the CLI"
  And mem_rule and ast_rule contain the unconditional "ALWAYS consult this skill" clause
```

#### Scenario: improvements skill contains no CLI invocations
```gherkin
Given the generated graphit-improvements SKILL.md
When I search for "graphit memory ", "graphit hub ", "graphit sync"
Then no CLI invocation remains (only anti-pattern warnings and prose)
  And graphit_hub_type-path, graphit_memory_insert, graphit_hub_submit are referenced
```

#### Scenario: new MCP tool resolves an artifact path
```gherkin
Given a running MCP server
When the client calls graphit_hub_type-path with type=skill and name=my-error-patterns
Then the result path contains "my-error-patterns"
```

## Progress Log

### 2026-07-21
- Added shared mandate builder + test (green).
- Strengthened all 5 mandate triggers; build green.
- Added memory + hub enforcement sections; aligned ast + knowledge wording.
- Added `graphit_hub_type-path` MCP tool + in-memory integration test (green).
- Converted improvements + dream CLI refs to MCP; added regression test (green).
- `go build ./...` and `go vet` clean; `make install` + `graphit sync` regenerated
  AGENTS.md and all skill files. Verified all enforcement present, no agent-facing CLI.
```
