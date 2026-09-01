# Task: Daemon — supervision by project activity window

**Date:** 2026-08-02
**Status:** ✅ Complete

## Problem

The global daemon (`internal/daemon`) discovers every project in `~/.graphit/global.lock.json`
via `ListActiveProjects()` — despite the name, that method only filters projects whose directory
still exists, not projects in recent use. On every discovery tick (30s by default), EVERY
registered project got a full, permanent `ProjectSupervisor`:

- `SyncModule` — an `fswatch` (inotify) watcher over the project's entire tree, forever
- `EmbeddingModule` — a scan every 2 minutes
- `DreamModule` — autonomous routine triggered by idleness

That means a developer with dozens of projects registered over time keeps dozens of filesystem
watchers and periodic loops running indefinitely, even for projects abandoned months ago —
exactly what the user reported: the daemon "keeps listening for changes in every project in the
global lock with no need for it".

## Solution

Introduced a **parking** mechanism based on an activity window:

1. **`internal/config/config.go`** — new function `ResolveProjectActivityWindow(inlineCfg,
   projectCfg) time.Duration`, reading the `daemon.activity_window` key through the standard
   resolution chain (inline → env → project → global → default). Default: 30 minutes. `"0"` turns
   parking off entirely (the previous behavior — always supervise).

2. **`internal/daemon/daemon.go`** — `Config.ProjectActivityWindow time.Duration` (zero value =
   off, preserving the previous behavior for anyone constructing `Daemon{}` directly, as the
   already existing tests do). `Daemon.parked map[string]ProjectInfo` holds projects that are
   registered but not supervised at the moment. `reconcileProjects` gained three new steps,
   all guarded by `window > 0`:
   - **Demotion**: a supervisor whose `IdleFor() >= window` is stopped (`sup.Stop()`) and the
     project migrates into `parked`.
   - **Promotion**: a project in `parked` (or never seen) is promoted to a full supervisor
     as soon as `projectRecentlyActive(dir, window)` — which reuses `dream.LastModifiedTime`,
     already used by the Dream module to decide idleness — finds a file touched within
     the window.
   - An error during the scan (inaccessible/empty directory) assumes "active" by default, so a
     project is never parked because of a failure in the check itself.

3. **`internal/daemon/project.go`** — `ProjectSupervisor` gained `lastActivity atomic.Int64` +
   `Touch()` / `IdleFor()`. New interface `ActivityReporter { SetActivityCallback(func()) }`:
   a module that implements it receives, when wired to the supervisor, the `sup.Touch` callback.
   That keeps `reconcileProjects` from having to re-scan the disk to know whether an *active*
   project is still active — what knows that in real time is `SyncModule` itself, which already
   receives every filesystem event.

4. **`internal/daemon/syncmodule.go`** — `SyncModule` implements `ActivityReporter`. Every batch
   of `fswatch` events (even if nothing in it is reindexable) calls `onActivity()`, resetting the
   supervisor's idleness clock.

5. **`cmd/graphit/commands/daemon.go`** — `runDaemonCore` resolves
   `cfg.ProjectActivityWindow = config.ResolveProjectActivityWindow(nil, nil)` from the
   global/env config, and the help text of the `daemon` command mentions the behavior.

Net effect: projects last touched more than `daemon.activity_window` ago no longer have a watcher,
an embedding loop, or a dream runner running — and they go back to being supervised automatically
on the next discovery tick after any change in them (the cost is a single `LastModifiedTime`
walk per parked project, per tick — not per active project).

## Trade-offs & Decisions

- **Resolving the window value happens once, at the CLI layer (`runDaemonCore`), not on every
  tick.** `reconcileProjects` only reads `d.cfg.ProjectActivityWindow`, a plain field — no config
  I/O per tick, and no dependency on `HOME` during the tests (which construct `Daemon{}`
  directly and therefore end up with a zero window = legacy, deterministic behavior).
- **No per-project override implemented** — `ResolveProjectActivityWindow` already accepts
  `projectCfg` (the same signature as every `Resolve*` in the `config` package), but
  `reconcileProjects` does not load each project's lockfile before deciding to supervise; only
  the builder (called already inside the "promoted" path) loads per-project config. Left out for
  scope — nobody has asked for a per-project pin yet.
- **Promoting parked projects costs one disk walk per tick (`dream.LastModifiedTime`, which
  already ignores `.git` and the brand directory).** There is no cheaper mechanism without
  reintroducing some kind of persistent watch — which would cancel out the gain. Accepted because
  the set of *parked* projects tends to be small (most active ones no longer pay this cost).

## System Knowledge

- `ListActiveProjects()` in `internal/hub/global_lock.go` is badly named: it filters only by the
  lockfile existing on disk, not by recent activity. The name (wrongly) suggested that this
  filter already existed before this task.
- `SyncModule` had already been migrated from `git status` polling to `fswatch` (inotify) — see
  the comment at the top of `Start()` in `syncmodule.go`. That means the cost "per registered
  project" before this change was inotify file descriptors/watch descriptors over the entire
  tree, not polling CPU.
- `dream.LastModifiedTime(dir)` (in `internal/dream/dream.go`) already existed so the Dream module
  could decide idleness — reused here as the only way to "probe" a project without keeping a
  watcher alive.

## Tests

- `internal/config/config_default_test.go` — `TestResolveProjectActivityWindow_{Default,
  ProjectOverride,EnvOverride,ZeroDisables,InvalidFallsBackToDefault}`.
- `internal/daemon/project_test.go` — `TestNewProjectSupervisor_StartsWithFreshIdleClock`,
  `TestProjectSupervisor_TouchResetsIdleFor`.
- `internal/daemon/daemon_reconcile_test.go` — `TestDaemon_ReconcileProjects_{ParksInactiveProject,
  PromotesActiveProject,DemotesIdleSupervisor,ActivityWindowDisabled_AlwaysSupervises,
  WiresActivityCallback}`.
- `internal/daemon/syncmodule_test.go` — `TestSyncModule_ImplementsActivityReporter`.
- Every test in `internal/daemon`, `internal/config` and `cmd/graphit/commands` passed
  with `-race -count=8` (the `daemon` package on its own) with no flakes after fixing a latent
  race in the new tests themselves (see below).

## Technical Debt

- [ ] No per-project override of `daemon.activity_window` (pinning). If a specific project needs
      to stay active regardless of prolonged silence, today the only option is turning parking
      off globally (`daemon.activity_window=0`).
- [ ] `daemon status` does not expose how many projects are `parked` vs supervised — it only reads
      the recent log from the file. That would be a natural observability improvement.

## Gotcha for whoever touches this module's tests

A test that launches `reconcileProjects` (which in turn does `go func(s){ s.Start(ctx, d.log)
}(sup)`) and calls `cancel()` **without first waiting for a signal that the module has already
run** runs a real risk of canceling the context before the supervisor's goroutine is even
scheduled — in that case `supervise()` falls into `if ctx.Err() != nil { return }` right on the
first iteration, never calling `entry.mod.Start()`. This is not environment flakiness: it
reproduced 100% of the time in a debug session (a goroutine dump via `runtime/pprof` confirmed
zero supervisor goroutines alive). Always synchronize through a "started" channel before
canceling — see the pattern in `TestDaemon_ReconcileProjects_PromotesActiveProject`.

## Modified Files

- `internal/config/config.go` — `ResolveProjectActivityWindow` + `defaultProjectActivityWindow`
- `internal/config/config_default_test.go` — tests for the function above
- `internal/daemon/daemon.go` — `Config.ProjectActivityWindow`, `Daemon.parked`, parking/promotion/
  demotion logic in `reconcileProjects`, `projectRecentlyActive`
- `internal/daemon/project.go` — `ActivityReporter`, `ProjectSupervisor.lastActivity/Touch/IdleFor`
- `internal/daemon/project_test.go` — tests for Touch/IdleFor
- `internal/daemon/daemon_reconcile_test.go` — tests for parking/promotion/demotion/wiring
- `internal/daemon/syncmodule.go` — `SetActivityCallback` + call on every batch
- `internal/daemon/syncmodule_test.go` — test for `ActivityReporter`
- `cmd/graphit/commands/daemon.go` — resolves `ProjectActivityWindow` from config, help text
