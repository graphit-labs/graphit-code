# Fix Self-Update Replacing Wrong Binary (Launcher Architecture)

## Objective

Fix the `self-update` command that was replacing the **core binary** instead of the **launcher binary**, causing version ping-pong after updates.

## Problem

The graphit binary uses a launcher architecture:
- **Launcher**: lightweight binary the user executes directly (e.g., `~/Downloads/graphit`)
- **Core**: heavy binary with all libs, extracted to `~/.graphit/runtime/<version>/graphit-core`

The launcher passes execution to the core via `syscall.Exec` (Unix) or `cmd.Run` (Windows).

When `self-update` ran inside the core, `os.Executable()` returned the **core binary path**, not the launcher. So `AtomicReplace` was replacing the core with the new launcher binary — an architecture mismatch that caused:

1. First `--version` after update: new launcher (mistakenly placed as core) starts, extracts its own runtime, reports new version ✓
2. Second `--version`: old launcher (never replaced) re-extracts old runtime (cleaning up the new one via `cleanupOldRuntimes`), reports old version ✗
3. Stable state: reverts to old version permanently

## Files Changed

| File | Change |
|------|--------|
| `cmd/graphit/commands/lifecycle.go` | Use `GRAPHIT_LAUNCHER_PATH` env var in self-update to target the launcher binary |

## Key Decisions

- **Use existing `LAUNCHER_PATH` env var**: The launcher already sets `GRAPHIT_LAUNCHER_PATH` with its resolved path (line 101 of `cmd/launcher/main.go`). The self-update now checks this env var and uses it as the replacement target.
- **No runtime cleanup in self-update**: Initially considered deleting the old runtime directory after replacing the launcher, but removed it because (a) Windows cannot delete DLLs loaded by the running process, and (b) the launcher's built-in `cleanupOldRuntimes` already handles this on next launch.
- **Fallback to `os.Executable()`**: If `LAUNCHER_PATH` is not set (e.g., running the core directly without the launcher, or dev builds), the self-update falls back to the original behavior.

## Cross-Platform Considerations

- **Linux/macOS**: `syscall.Exec` replaces the launcher process with the core. `os.Executable()` returns the core path. Fix correctly redirects to launcher via env var.
- **Windows**: `syscall.Exec` is not available; launcher uses `cmd.Run()` and stays alive as parent. `os.Rename` works on running executables in Windows. The `.bak` cleanup may fail (file in use) but is already silently ignored.
- **All platforms**: Env vars are universal. No file deletion of in-use files.

## Use Cases

### UC-01: Self-Update via Launcher (Primary)
- **Actor**: User running graphit via the launcher binary
- **Preconditions**: Launcher sets `GRAPHIT_LAUNCHER_PATH` env var; newer version available
- **Main Flow**:
  1. User runs `./graphit self-update`
  2. Launcher starts, sets `GRAPHIT_LAUNCHER_PATH`, exec's core
  3. Core's self-update reads `GRAPHIT_LAUNCHER_PATH`
  4. Downloads new launcher binary
  5. Replaces the launcher at the path from env var
  6. On next run, new launcher extracts its runtime and runs new core
- **Postconditions**: Launcher binary is updated; next run uses new version consistently
- **Affected files**: `cmd/graphit/commands/lifecycle.go`

### UC-02: Self-Update Without Launcher (Fallback)
- **Actor**: Developer running core binary directly (dev builds)
- **Preconditions**: `GRAPHIT_LAUNCHER_PATH` is not set
- **Main Flow**:
  1. User runs `./graphit-core self-update`
  2. `os.Executable()` returns the core path
  3. No launcher env var → falls back to replacing `os.Executable()` result
- **Postconditions**: Core binary is replaced directly (original behavior)
- **Affected files**: `cmd/graphit/commands/lifecycle.go`

## Test Cases & Acceptance Criteria

### TC-01: Self-update replaces launcher when LAUNCHER_PATH is set (Ref: UC-01)
- **Given** the `GRAPHIT_LAUNCHER_PATH` env var is set to `/path/to/launcher`
- **When** self-update runs successfully
- **Then** `AtomicReplace` is called with target = `/path/to/launcher`

### TC-02: Self-update falls back to os.Executable when LAUNCHER_PATH is not set (Ref: UC-02)
- **Given** the `GRAPHIT_LAUNCHER_PATH` env var is not set
- **When** self-update runs successfully
- **Then** `AtomicReplace` is called with target = resolved `os.Executable()` path

### TC-03: Version is consistent after self-update (Ref: UC-01)
- **Given** user runs `./graphit self-update` and it succeeds
- **When** user runs `./graphit --version` multiple times
- **Then** all invocations report the new version consistently

### TC-04: Cross-platform: Windows DLLs not deleted during update (Ref: UC-01)
- **Given** self-update runs on Windows
- **When** the launcher is replaced
- **Then** no attempt is made to delete the runtime directory (DLLs in use)
- **And** the old runtime is cleaned up by the launcher on next run
