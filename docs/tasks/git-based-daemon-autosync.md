# Task: Git-Based Daemon Auto-Sync & Watcher Overhaul

## Objective

Replace all `fsnotify`-based file watchers with efficient git polling (`git status --porcelain`), and make the daemon automatically reindex AST, knowledge wiki, and memory wiki when source files change.

## Problems Solved

1. **Daemon not auto-syncing**: The daemon had no file watching — when project files changed, nothing happened automatically.
2. **fsnotify resource exhaustion**: The old watcher consumed one file descriptor per directory, recursively. With many projects, this exhausted system limits.
3. **SSH hang on init**: Git operations hung on machines where the remote host wasn't in `known_hosts`.
4. **Commit-between-polls race condition**: If changes were committed between two polls, `git status` appeared clean both times and the change was missed. Fixed by combining `git rev-parse HEAD` (detects new commits) with `git status` (detects uncommitted changes) into a single state hash.

## Files Changed

### New Files

- `internal/daemon/syncmodule.go` — Per-project SyncModule that polls `git status` and triggers AST + knowledge reindexing.
- `internal/daemon/memorysyncmodule.go` — Global MemorySyncModule that watches memory git worktrees and recompiles memory wiki.

### Modified Files

- `internal/ast/watcher.go` — Rewrote from `fsnotify` to git polling. Zero file descriptors, respects `.gitignore` + `.astignore`.
- `cmd/graphit/commands/runners.go` — Converted `watchAndReindex()` from `fsnotify` to git polling. Removed `fsnotify` import entirely.
- `cmd/graphit/commands/daemon.go` — Registered `SyncModule` per-project and `MemorySyncModule` globally.
- `internal/git/cli_backend.go` — Changed SSH from `StrictHostKeyChecking=accept-new` to `BatchMode=yes`. Added `wrapSSHError` to detect SSH host key failures and show user-friendly guidance (`ssh -T`).

## Architecture

### Watch Strategy: Git Polling

All watchers use `git status --porcelain -unormal`:
- **Zero file descriptors** — just spawns `git` periodically
- **Respects `.gitignore`** automatically (git handles it)
- **Respects `.astignore`** via the existing `ignorer.IgnoreChecker`
- **`-unormal`** groups untracked directories (faster than `-uall`)

### Daemon Module Structure

| Module | Scope | Source Watched | Output | Poll/Debounce |
|---|---|---|---|---|
| `SyncModule` (AST) | Per-project | Project code (git repo) | ladybugdb | 5s / 1s |
| `SyncModule` (Knowledge) | Per-project | Configurable docs dir (git repo) | Knowledge wiki | 5s / 1s |
| `MemorySyncModule` | Global | Memory worktrees (git repos) | Memory wiki | 10s / 1s |

### Config Resolution

All config values respect the override chain: inline → env → project lockfile → global config → compiled defaults.
- `knowledge.docs_dir` — source directory for knowledge indexing
- `ast.index_source` — whether to store source code in AST graph
- `modules.sync` — disable sync module per-project
- `modules.ast` / `modules.knowledge` — disable specific reindexing

### SSH Error Handling

Instead of auto-accepting unknown SSH hosts:
- Uses `BatchMode=yes` to reject unknown hosts immediately
- `wrapSSHError` detects host key verification failures
- Returns actionable error: "Verify the host manually: ssh -T git@hostname"

## Key Decisions

1. **Git polling over fsnotify**: Trade ~3.5s average detection delay for zero file descriptor usage. Acceptable for background daemon.
2. **Debounce 1s**: Fast enough for interactive use while avoiding redundant reindexes during rapid saves.
3. **Memory as global module**: Memory worktrees live in `~/.graphit/`, not per-project. Separate module avoids duplicating watches.
4. **`-unormal` over `-uall`**: Groups untracked directories for faster git status. We only need hash comparison, not individual file lists.
5. **SSH reject over auto-accept**: Security-first approach with actionable error messages.

## Use Cases

### UC-01: Daemon detects project file change and reindexes
- **Actor**: Developer editing code
- **Preconditions**: Daemon running, project registered in `global.lock`
- **Main flow**: Developer saves file → SyncModule polls git status every 5s → hash changes → 1s debounce → reindex AST + knowledge
- **Alternative flow**: File is in `.gitignore` or `.astignore` → filtered out → no reindex
- **Postconditions**: AST graph and knowledge wiki are up-to-date

### UC-02: Daemon detects memory change and recompiles wiki
- **Actor**: AI agent inserting memory via MCP
- **Preconditions**: Daemon running, memory git store initialized
- **Main flow**: Agent calls `memory_insert` → worktree file changes → MemorySyncModule polls → hash changes → recompile wiki
- **Postconditions**: Memory wiki reflects new memory

### UC-03: SSH fails on new machine
- **Actor**: Developer running `graphit init` on new machine
- **Preconditions**: Remote host not in `known_hosts`
- **Main flow**: Git operation fails → `wrapSSHError` detects host key error → returns message with `ssh -T` guidance
- **Postconditions**: User sees actionable error, can verify host manually

### UC-04: CLI watch commands (ast watch, knowledge watch, memory watch)
- **Actor**: Developer running CLI watch
- **Preconditions**: Project initialized
- **Main flow**: CLI polls git status every 2s → detects change → 500ms debounce → reindex
- **Postconditions**: Index stays current while developer works

## Test Cases & Acceptance Criteria

### TC-01: Build succeeds (Ref: all UCs)
- **Given** all changes applied
- **When** `go build ./...`
- **Then** compiles with zero errors ✅

### TC-02: All tests pass (Ref: all UCs)
- **Given** all changes applied
- **When** `go test ./internal/ast/... ./internal/daemon/... ./internal/git/... ./cmd/graphit/commands/...`
- **Then** all tests pass ✅

### TC-03: No fsnotify references remain (Ref: UC-01, UC-04)
- **Given** all changes applied
- **When** `grep -r fsnotify *.go`
- **Then** zero results ✅

### TC-04: Config resolution uses project config (Ref: UC-01)
- **Given** project with `knowledge.docs_dir=documentation` in lockfile
- **When** SyncModule runs reindexKnowledge
- **Then** uses `documentation/` as source dir, not default `docs/`
