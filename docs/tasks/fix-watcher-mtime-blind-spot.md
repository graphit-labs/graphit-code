# Fix Watcher Mtime Blind Spot

## Objective

Resolve a detection gap in all git-polling watchers where re-edits to already-dirty files were not triggering reindexing. The `git status --porcelain` output is identical for a file that is modified once vs. modified again with different content (both show ` M file.go`), so the state hash was unchanged and the watcher missed the second edit.

## Root Cause

Two problems in the composite state hash:

1. **Porcelain text is content-blind**: `git status --porcelain` only reports status codes (M, A, D, ??) and paths — not content hashes or timestamps. Two different modifications to the same file produce identical porcelain output.

2. **`-unormal` hid individual untracked files**: Untracked directories were grouped as `?? dir/` instead of listing each file. This meant files in new directories couldn't be individually detected, stat'd for mtime, or returned by `changedFiles()` for reindexing.

## Solution

Two combined fixes:

### 1. Include file mtime in the state hash

For each file listed in the porcelain output, `os.Stat()` provides the modification timestamp. The new formula is:
```
SHA256(HEAD + "\n" + porcelain + "\n" + file1:mtime1\nfile2:mtime2\n...)
```

Performance characteristics:
- **One `os.Stat()` syscall per dirty file** — no extra git processes
- Typically <10 dirty files in a working tree, so cost is negligible
- `UnixNano()` precision captures sub-second edits on modern filesystems
- Deleted files return `os.Stat` error and are skipped (deletion is already captured in porcelain change)

### 2. Switch from `-unormal` to `-uall`

All `git status --porcelain` calls now use `-uall`, which lists every untracked file individually instead of grouping by directory. This ensures:
- Every file (tracked or untracked) appears individually in the porcelain output
- `dirtyFileMtimes()` can stat every dirty file for mtime
- `changedFiles()` can return individual untracked files for reindexing
- Cost is negligible with a properly configured `.gitignore`

## Files Changed

- **`internal/ast/watcher.go`** — Added `dirtyFileMtimes()` helper; modified `statusHash()` to include mtimes; switched to `-uall` in both `statusHash()` and `changedFiles()`
- **`internal/daemon/syncmodule.go`** — Added `dirtyFileMtimes()` helper; modified `gitStateHash()` to include mtimes and use `-uall`
- **`internal/daemon/memorysyncmodule.go`** — Modified `memoryWorktreeHash()` to include mtimes and use `-uall` (uses `dirtyFileMtimes` helper from syncmodule.go)
- **`cmd/graphit/commands/runners.go`** — Added `cliDirtyFileMtimes()` helper; modified `cliWatchHash()` to include mtimes and use `-uall`

## Key Decisions

- **mtime over `git hash-object`**: `os.Stat` is a single syscall per file — faster than spawning any git process. `git hash-object` would provide content-based detection but at subprocess cost per poll.
- **`-uall` over `-unormal`**: `-unormal` grouped untracked directories, creating a blind spot where files inside new directories could not be individually stat'd or reindexed. `-uall` lists every file, closing the gap. Performance impact is negligible with proper `.gitignore`.
- **`UnixNano` precision**: Maximizes sensitivity. Filesystems with lower precision (e.g., FAT32 at 2s) simply have zeros in the nanosecond part — still functional.
- **Duplicated helper across packages**: `dirtyFileMtimes` exists in both `ast` and `daemon` packages. The function is small (~15 lines) and avoids cross-package coupling.

## Use Cases

### UC-01: Re-edit of Already Dirty Tracked File Triggers Reindex

- **Actor**: Developer editing code
- **Preconditions**: Tracked file `A.go` is already modified (dirty in git)
- **Main Flow**:
  1. Watcher polls and computes state hash (includes mtime of `A.go`)
  2. Developer saves `A.go` with different content
  3. `os.Stat(A.go).ModTime()` changes → mtime string changes → hash changes
  4. Watcher detects hash mismatch → debounce → reindex
- **Postconditions**: The AST graph reflects the latest content of `A.go`
- **Affected files**: `internal/ast/watcher.go`, `internal/daemon/syncmodule.go`

### UC-02: Re-edit of Untracked File in New Directory

- **Actor**: Developer creating and editing files in a new package
- **Preconditions**: New directory `pkg/` with `handler.go` exists, not yet `git add`'d
- **Main Flow**:
  1. With `-uall`, `git status` shows `?? pkg/handler.go` (individual file, not `?? pkg/`)
  2. Watcher computes hash including mtime of `pkg/handler.go`
  3. Developer edits `handler.go` again
  4. mtime changes → hash changes → reindex triggered
- **Postconditions**: AST graph includes latest content of `handler.go`
- **Affected files**: Same as UC-01

### UC-03: Multiple Rapid Saves to Same File

- **Actor**: Developer editing with auto-save
- **Preconditions**: File `B.go` is dirty, watcher has debounced once
- **Main Flow**:
  1. Each save updates `B.go` mtime
  2. Poll detects hash change, enters debounce
  3. Debounce sub-polls also include mtime → timer keeps resetting
  4. After stabilization, reindex runs once with final content
- **Postconditions**: Single reindex with latest state
- **Affected files**: Same as UC-01

### UC-04: No False Negatives on File Deletion

- **Actor**: Developer deleting a dirty file
- **Preconditions**: File `C.go` is dirty
- **Main Flow**:
  1. Developer deletes `C.go`
  2. `git status` changes from ` M C.go` to ` D C.go` → porcelain changes
  3. `os.Stat(C.go)` returns error → skipped in mtime computation
  4. Hash changes due to porcelain difference → reindex triggered
- **Postconditions**: AST graph removes `C.go` entities
- **Affected files**: Same as UC-01

## Test Cases & Acceptance Criteria

### TC-01: Build Success (Ref: all UCs)
- **Given** the mtime and -uall changes are applied to all four files
- **When** `go build ./...` is executed
- **Then** the build succeeds with zero errors

### TC-02: Unit Tests Pass (Ref: all UCs)
- **Given** the changes
- **When** `go test ./internal/ast/... ./internal/daemon/... ./cmd/graphit/commands/...` is run
- **Then** all tests pass

### TC-03: Hash Changes on Re-edit (Ref: UC-01, UC-02)
- **Given** a dirty file `X.go` and a recorded hash H1
- **When** `X.go` is modified again (new content, same git status output)
- **Then** the new hash H2 ≠ H1 (because mtime changed)

### TC-04: Hash Stable When No Changes (Ref: UC-01)
- **Given** a dirty file `X.go` and no further edits
- **When** hash is computed twice in sequence
- **Then** both hashes are identical

### TC-05: Deleted Files Don't Cause Errors (Ref: UC-04)
- **Given** a file listed in porcelain that no longer exists on disk
- **When** `dirtyFileMtimes` processes the porcelain
- **Then** the deleted file is skipped (no panic, no error)

### TC-06: Untracked Files Listed Individually (Ref: UC-02)
- **Given** `-uall` flag is used
- **When** a new directory `pkg/` with `handler.go` exists (not git-added)
- **Then** `git status --porcelain -uall` shows `?? pkg/handler.go` (not `?? pkg/`)
