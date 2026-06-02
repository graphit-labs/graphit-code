# Task: Comprehensive Unit Tests for internal/daemon

## Summary
Added comprehensive unit tests for the `internal/daemon` package, raising code coverage from **0.8%** to **29.7%**.

## Changes

### Test Files Created/Modified

- `daemon_test.go` — Rewrote with tests for `DefaultConfig`, `GlobalDaemonDir`, `New` (default/custom/negative intervals), `event` (with/without callback), `stampChanged` (all branches), `log` (nil file / writes), `closerFunc`, and version check helpers.
- `pidfile_test.go` — **New**. Tests for `PIDFile.Write`/`Read` cycle, `Read` with missing file, malformed content (table-driven), `Remove`, `IsAlive` (live/dead/no-file), `Signal`/`SignalOS` edge cases, and directory auto-creation.
- `module_test.go` — **New**. Tests for `ModuleState.String` (all states + unknown), `moduleEntry` construction, `setState`, `setError` (including nil), `setStarted`, `incRestarts`/`resetRestarts`, `status`, `String`, concurrent access race test, and `runProtected` (normal/error/panic recovery).
- `scheduler_test.go` — **New**. Tests for `cronMarker`, `resolveExePath`, `removeCronEntry` (no marker, with marker, at end, empty, only marker, multiple occurrences), `IsSchedulerInstalled` smoke test.
- `project_test.go` — **New**. Tests for `newProjectSupervisor` (with modules, empty), `AddCloser`, `Status` reporting, `Stop` (with/without cancel), `projectLog` (nil file, with global fn).
- `embedserver_test.go` — **New**. Tests for `EmbedPortFile` path, `NewEmbedServer` port file config, `EmbedServerModule.Name`.
- `syncmodule_test.go` — **New**. Tests for `dirtyFileMtimes` (empty, short lines, directories, valid file, nonexistent), `SyncModule.Name`, `worktreeDirForBranch`, `parseBranch` (table-driven), `MemorySyncModule.Name`.
- `adapters_test.go` — **New**. Tests for `EmbeddingModule` (default/custom/negative interval, name), `DreamModule` (construction, name).

### Test Strategy
- Focused on **pure functions** and **unit-testable logic** without external dependencies.
- Used `t.TempDir()` for filesystem tests (auto-cleaned).
- Used table-driven tests for `ModuleState.String`, `PIDFile.Read_Malformed`, `parseBranch`, `worktreeDirForBranch`, `removeCronEntry`.
- Concurrent access race test for `moduleEntry` (verified with `-race`).
- Panic recovery test for `runProtected`.

### What Was Excluded
- Functions requiring `git` (e.g., `gitStateHash`, `SyncModule.Start`, `MemorySyncModule.Start`).
- Functions requiring network/embedding models (e.g., `EmbedServer.Start`, HTTP handlers).
- Functions spawning OS processes (e.g., `EnsureRunning`, `InstallScheduler`).

## Results
```
ok  github.com/graphit-labs/graphit-code/internal/daemon  1.231s  coverage: 29.7% of statements
```

All tests pass with `-race -count=1`.
