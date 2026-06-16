---
title: Enrich AGENTS.md Mandate Triggers with Comprehensive Instructions
status: done
created: 2026-06-16
updated: 2026-06-16
tags: [mandates, agents, rules, instructions]
---

# Enrich AGENTS.md Mandate Triggers

## Objective

Expand the `MandateTrigger()` functions in all 5 modules (memory, ast, hub, knowledge, improvements)
from compressed single-paragraph summaries to comprehensive, multi-section markdown blocks
that properly guide AI agents — restoring the level of detail that existed in the old HTML block format.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/memory/rule.go` | Modified `MandateTrigger()` | Expanded from 1-paragraph to full markdown with session start protocol, activation triggers, quick reference, key rules, subagent propagation |
| `internal/ast/rule.go` | Modified `MandateTrigger()` | Expanded with grep→AST translation table, quick reference, property reference, key rules, subagent propagation |
| `internal/hub/rule.go` | Modified `MandateTrigger()` | Expanded with activation triggers, quick reference, critical rule, subagent propagation |
| `internal/knowledge/rule.go` | Modified `MandateTrigger()` | Expanded with post-change protocol, quick reference, subagent propagation |
| `internal/improvements/rule.go` | Modified `MandateTrigger()` | Expanded with dream subjects, critical rules, subagent propagation |

## Key Decisions

- Kept XML tag format (`<mem_rule>`, `<ast_rule>`, etc.) — only changed the content inside them
- Used raw string literals (backtick strings) in Go for readability of multi-line markdown
- Matched all sections from the user's old HTML block version
- AGENTS.md grew from ~3KB/13 lines to ~16KB/274 lines

## Notes

- The `UpsertMandateTrigger` mechanism already supported multi-line content — no infrastructure changes needed
- Tree-sitter test failures in `internal/ast` are pre-existing (missing `.so` grammar files) and unrelated to this change
