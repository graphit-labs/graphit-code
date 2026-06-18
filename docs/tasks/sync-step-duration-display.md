# Task: Display Step Duration in sync Output

**Date:** 2026-06-18  
**Status:** Complete

## Objective

Each step printed during `graphit sync` should include its elapsed duration so
the user can see at a glance how long each phase took.

**Desired output example:**
```
✓ AST: 584 files up to date (0.1s)
✓ Knowledge wiki reindexed (3.2s)
✓ Memory repository synced (0.8s)
✓ Memory wikis reindexed (1.4s)
✓ Hub repository synced (0.5s)
✓ IDE rules updated (0.0s)
✓ IDE adapter synced (0.0s)
✓ Git hooks synced (0.0s)
✓ Sync complete
✓ Vector embeddings generated (12.3s)
✓ Events synced (0.2s)
✓ Hub artifacts reconciled (0.4s)
✓ Memory maintenance complete (1.1s)
```

## Implementation

### File changed: `internal/output/printer.go`

**Approach:** Instead of adding timing at every individual call site, the
`output.Task` struct itself now tracks when it was started and automatically
appends `(Xs)` to every `Done` and `Fail` message. This means all tasks
across the entire codebase benefit from timing with zero call-site changes.

**Changes:**
- Added `startedAt time.Time` field to `Task` struct.
- Set `startedAt = time.Now()` in `StartTask`.
- In `Done`: appended `dim.Sprintf(" (%.1fs)", elapsed.Seconds())` to the
  formatted message before printing.
- In `Fail`: same elapsed suffix so failures also show how long they ran.

The dim color styling keeps the timing visually subordinate to the main
message text.

## Verification

- `go build ./internal/output/...` — OK
- `go build ./cmd/graphit/...` — OK (pre-existing third-party C warning only)
