Task: Supervising Projects' Activity Windows with a Daemon

**Date:** 2026-08-02
**Status:** ✅ Complete

## Problema

The global daemon (`internal/daemon`) discovers all the projects of `~/.graphit/global.lock.json` via `ListActiveProjects()` — despite its name, this method only filters out projects whose directory still exists, not those in use recently. Every tick of discovery (30 seconds by default), each registered project gained a `ProjectSupervisor` complete and permanent:

- `SyncModule` — an inotify watcher on the entire project tree, always
- `fswatch` — every two minutes
- `EmbeddingModule` — autonomous routine run by idle time


This means that a developer with dozens of projects registered over time
maintains dozens of filesystem watchers and periodic loops running indefinitely,
even for abandoned projects from months ago — exactly what the user reported:
the daemon "continues to listen for changes on all projects in the global lock without needing."

Solution

A mechanism of **parking** (parking lot) by window activity:

**INLINE_8** — a new function `ResolveProjectActivityWindow(inlineCfg, projectCfg)` resolves the standard chain of resolution (inline → env → project → global → default). Default: 30 minutes. **INLINE_10** turns off parking completely (previous behavior—always supervises).

2. **`internal/daemon/daemon.go`** — `Config.ProjectActivityWindow time.Duration` (zero value =
   off, preserving the previous behavior for those constructing `Daemon{}` directly,
   as existing tests do). `Daemon.parked map[string]ProjectInfo` stores projects registered but not supervised at that moment. `reconcileProjects` gained three new steps:
   - **Demo**: supervisor whose `IdleFor() >= window` is paused (`sup.Stop()`) and the project migrates to `parked`.
   - **Promotion**: project in `parked` (or never seen) promoted to complete supervisor as soon as `projectRecentlyActive(dir, window)` — which reuses `dream.LastModifiedTime`, already used by the module Dream for deciding on idle status — finds a file touched within the window.
   - Error during scan (directory inaccessible/vacant) defaults to "active" by default, never stopping a project due to its own check.

3. **`internal/daemon/project.go`** — `ProjectSupervisor` gained `lastActivity atomic.Int64` +
   `Touch()` / `IdleFor()`. New interface `ActivityReporter { SetActivityCallback(func()) }`:
   a module that it implements receives the callback `sup.Touch` when connected to the supervisor.
   This prevents `reconcileProjects` from needing to re-varnish the disk to know if a project
   *active* is still active — who knows, maybe this real-time knowledge is already provided by itself `SyncModule`, which already receives all filesystem events.

4. **`internal/daemon/syncmodule.go`** — `SyncModule` implements `ActivityReporter`. All batches of events from `fswatch` (even if none of it is reindexable) call `onActivity()`, resetting the supervisor's clock cycle timer.

5. **Inline 37** resolves from the
   inline 38 configuration global/env, and the help text for command Inline 40 mentions the behavior.

Effectively, projects that have been touched more than 41 INLINE_41 days ago no longer have watchers,
embedding loop or dream runner running — and automatically resume supervision on the next discovery tick after any changes to them (the cost is a single `LastModifiedTime` walk per stationary project per tick — not by active project).

Trade-offs & Decisions

- **The resolution of the window value happens once, in the CLI layer (`runDaemonCore`), not every tick.** `reconcileProjects` only reads `d.cfg.ProjectActivityWindow`, a pure field — without I/O configuration per tick or depending on `HOME` during tests (which build `Daemon{}` directly and therefore have window zero = legacy behavior, deterministic). - **Without project-specific override implemented** — `ResolveProjectActivityWindow` already accepts `projectCfg` (same signature as all `Resolve*` in package `config`), but `reconcileProjects` does not load the lockfile of each project before deciding to supervise; only the builder (called already within the "promoted" path) loads configuration per project. It's out of scope — no one has requested a pinned project yet.
- **Promoting stalled projects incurs a disk walk every tick (`dream.LastModifiedTime`, which ignores `.git` and the brand directory).** There is no more cost-effective mechanism without reintroducing some form of persistent watch — which would negate the gain. I accept because the set of *stalled* projects tends to be small (most assets do not pay this cost).

## System Knowledge

- _INLINE_55__ in _INLINE_56__ is poorly named: it filters only by the existence of the lockfile on disk, not by recent activity. The name suggested (incorrectly) that this filter already existed before this task.
- `SyncModule` had been migrated from polling `git status` to `fswatch` (inotify) — see comment at the top of `Start()` in `syncmodule.go`. This means that "per project" overhead before this change was file descriptors/watch descriptors for inotify over the entire tree, not CPU from polling.
- `dream.LastModifiedTime(dir)` (in _INLINE_63__) already existed to allow Dream module to decide on idle — reapplied here as the only way to "ring" a project without keeping a watcher alive.

## Testes

- `internal/config/config_default_test.go` — `TestResolveProjectActivityWindow_{Default, ProjectOverride, EnvOverride, ZeroDisables, InvalidFallsBackToDefault}`.
- `internal/daemon/project_test.go` — `TestNewProjectSupervisor_StartsWithFreshIdleClock`, `TestProjectSupervisor_TouchResetsIdleFor`.
- `internal/daemon/daemon_reconcile_test.go` — `TestDaemon_ReconcileProjects_{ParksInactiveProject, PromotesActiveProject, DemotesIdleSupervisor, ActivityWindowDisabled_AlwaysSupervises, WiresActivityCallback}`.
- `internal/daemon/syncmodule_test.go` — `TestSyncModule_ImplementsActivityReporter`.
- All tests for `internal/daemon`, `internal/config`, and `cmd/graphit/commands` passed with `-race -count=8` (the package `daemon` alone) without flakes after fixing a latent race in the very same new tests.


## Technical Debt

- [ ] Without project-specific override by design (INLINE_76). If a specific project needs to remain always active independently of prolonged silence, today only globally un-pins the parking (INLINE_77).
- [ ] INLINE_78 does not expose how many projects are supervised vs. pinned — it just reads the recent log file. This would be an observable improvement.

Note: The inline codes and underscores have been kept as they were in Portuguese to preserve the original structure and meaning of the text.

Gotcha for anyone messing with this module's tests

A test that launches `reconcileProjects` (which in turn executes `go func(s){ s.Start(ctx, d.log)
}(sup)`) e chama `cancel()` **without waiting for a signal indicating the module has already started** runs the risk of prematurely canceling the context before the supervisor's goroutine is even scheduled — this results in `supervise()` falling into `if ctx.Err() != nil { return }` immediately on the first iteration, never calling `entry.mod.Start()`. This is not flaky environment behavior: reproducing 100% of the time in a debug session (the supervisor’s goroutines were confirmed to be dead via `runtime/pprof`). Always synchronize via a "started" channel before canceling — see the pattern in `TestDaemon_ReconcileProjects_PromotesActiveProject`.

Note: The inline codes and references are placeholders for actual code snippets or identifiers that should be replaced with their corresponding content.

## Arquivos Modificados

- `internal/config/config.go` + `ResolveProjectActivityWindow` = ___INLINE_89__
- `internal/config/config_default_test.go` = tests of the function above
- `internal/daemon/daemon.go` = `Config.ProjectActivityWindow`, `Daemon.parked`, logic for parking/promotion/demonstration in `reconcileProjects`, `projectRecentlyActive`
- `internal/daemon/project.go` = `ActivityReporter`, `ProjectSupervisor.lastActivity/Touch/IdleFor`
- `internal/daemon/project_test.go` = tests of Touch/IdleFor
- `internal/daemon/daemon_reconcile_test.go` = tests of parking/promotion/demonstration/wiring
- `internal/daemon/syncmodule.go` = `SetActivityCallback` + call in each batch
- `internal/daemon/syncmodule_test.go` = test of `ActivityReporter`
- `cmd/graphit/commands/daemon.go` = resolve `ProjectActivityWindow` from config, help text
