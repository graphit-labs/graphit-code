# Task: Remove Unnecessary Comments

**Date:** 2026-06-06
**Status:** Complete
**CI:** ✅ `make ci` passed

## Summary

Audited all ~80 Go source files across the entire codebase (27 internal modules + 3 cmd packages) for unnecessary, obvious, legacy, and misplaced comments. Removed 51 redundant comments and 1 dead code block across 16 files.

## Changes

### Comment Removals (51 total)

| File | Removed | Reason |
|------|---------|--------|
| `internal/ai/cli.go` | 6 comments + 1 dead code block | Obvious restating of code intent |
| `internal/ai/embedding_proxy.go` | 1 | Self-evident socket check |
| `internal/ast/composite_parser.go` | 2 | Condition already expresses intent |
| `internal/ast/grammar_loader.go` | 6 | Godoc repeating function names |
| `internal/ast/query_loader.go` | 20 | Godoc repeating function names + numbered inline comments restating docstrings |
| `internal/daemon/autostart.go` | 1 | Restated by 5-line docstring above |
| `internal/hub/adapters/ide/base.go` | 1 | Context already clear |
| `internal/memory/appsvc.go` | 1 | Name + parameter self-documenting |
| `internal/knowledge/community.go` | 1 | Name + return type self-documenting |
| `internal/knowledge/lint.go` | 3 | Type names self-documenting |
| `internal/knowledge/staleness.go` | 1 | Name self-documenting |
| `internal/knowledge/wiki.go` | 1 | Struct name + fields self-documenting |
| `internal/slogutil/slogutil.go` | 1 | Idiomatic size check is self-evident |
| `internal/uiserver/unified_server.go` | 2 | Route handlers self-documenting |
| `internal/uiserver/daemon_dream_handler.go` | 1 | SIGKILL is self-evident |
| `internal/wikisvc/wikisvc.go` | 2 | DTO struct fields self-documenting |

### Dead Code Removal

- `internal/ai/cli.go`: Removed no-op if-block (`if returnedSessionID == "" && spec.sessionFlag != "" { returnedSessionID = "" }`) — leftover from planned session extraction feature.

## What Was NOT Changed

- All SAFETY/NOTE/DECISION markers preserved
- All section dividers (`// -----------`) preserved
- All build tags and compiler directives preserved
- All lock-scope documentation preserved (mutex comments in daemon)
- All comments explaining *why* (non-obvious gotchas) preserved
- No documentation was relocated to `docs/` — audit found zero candidates

## Findings

The codebase was already remarkably clean. The `No_obvious_or_redundant_comments` memory convention is well-followed. The main offenders were godoc-style comments that simply repeated the function/type name.
