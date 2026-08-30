---
title: Optimize IDE Adapter and IDE Rule Sync Performance
status: done
created: 2026-06-18
updated: 2026-06-18
tags: [performance, sync, ide-adapter, idempotency]
---

# Optimize Sync Performance: IDE Adapter and IDE Rule

## Objective

`graphit sync` (and the `graphit_sync` MCP) was slow even when nothing in the project had
changed. The "Updating IDE rules" and "Syncing IDE adapter" steps performed unnecessary I/O
(reading and rewriting files) on every invocation, regardless of any actual change.

## Implementation Details

### Problem 1 — `UpsertMandateTrigger` ran legacy cleanup before checking idempotency

**File**: `internal/hub/adapters/ide/mandate.go`

**Before**: The function always ran `cleanupLegacy(targetPath)` before checking whether the
trigger content was already correct. `cleanupLegacy` reads and potentially rewrites AGENTS.md 3
times (once per legacy block), and it was called 5 times per sync (once per module: knowledge,
ast, hub, memory, improvements).

**After**:
1. Reads the file once at the start
2. Checks whether legacy blocks are present (string search, no destructive I/O)
3. Checks whether the trigger content is already correct
4. If there's no legacy content AND the content is already correct → **return nil** (zero writes)
5. Only runs `cleanupLegacy` if needed

**Functions added**:
- `readMandateContentFromString(content string) string` — extracts the inner content of the
  mandate block from an already-read string, avoiding re-reading the disk

**Fix in the legacy marker**: The detection format was wrong (`START`/`END` in the name),
corrected to match the actual `HTMLBlockStyle` format: `<!-- MARKER -->` / `<!-- END MARKER -->`.

### Problem 2 — `copyArtifact` folder-mode always did RemoveAll+copyDirAll

**File**: `internal/hub/adapters/ide/base.go`

**Before**: For `skill`-type artifacts (folder-mode), the code always did:
```go
_ = os.RemoveAll(dest)   // destroys the destination
return copyDirAll(...)   // re-copies everything
```
`copyDirAll` uses `copyFile` with idempotency (compares size+content), but since `RemoveAll`
destroyed the destination beforehand, idempotency never actually kicked in.

**After**: Before `RemoveAll`, it calls `dirContentsEqual(src, dst)`. If the directories are
identical (same tree, same sizes, same content), it returns nil immediately.

**Function added**:
- `dirContentsEqual(src, dst string) bool` — compares two directories using `filepath.Walk`:
  verifies that every file in src exists in dst with the same size and content, and that dst has
  no extra files. Uses `filepath.SkipAll` to short-circuit on divergence.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go` | Modified | Idempotency before legacy cleanup; `readMandateContentFromString` helper |
| `internal/hub/adapters/ide/base.go` | Modified | `dirContentsEqual` for folder-mode; skip RemoveAll+copy when identical |

## Key Decisions

- **Single file read**: Instead of reading the file multiple times (once for legacy cleanup, once
  to read the mandate), we now read it once and pass the content as a string to the helpers.
- **String search for legacy detection**: Using `strings.Contains` is O(n) but only runs once,
  versus compiling a regex and running `RemoveBlockStyled` 3 times (which includes
  reading+writing).
- **File counting for extra dst files**: The directory-equality check does two additional walks
  just to count files. This could be combined, but readability is better this way, and the cost
  is minimal compared to RemoveAll+copy.

## Notes

- The `null character(s) preserved in literal` warning in the build comes from the third-party
  package `go-tree-sitter/lua` — irrelevant and pre-existing.
- `filepath.SkipAll` (added in Go 1.20) is available in the Go version used by the project.
- All tests in the `internal/hub/adapters/ide` package pass without modification.
