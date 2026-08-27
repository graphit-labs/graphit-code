# Daemon stderr stops going to /dev/null

**Date:** 2026-07-31
**Scope:** `internal/daemonctl/daemonctl.go`, `internal/daemon/autostart.go`,
`cmd/graphit/commands/daemon.go`, `internal/daemonctl/stderr_log_test.go`
**Origin:** a daemon stopped making progress and there was nothing to read

---

## What happened

After a `make install`, the daemon restarted correctly and sat for 40 minutes at 7.4 GB
RSS without writing a byte. Measured: `/proc/<pid>/io` counters frozen, no child
process, main thread in `futex_do_wait`, and the project's `ladybugdb` never created. Not
slow — stuck.

`SIGQUIT`, which makes the Go runtime dump every goroutine's stack, produced nothing:

```
/proc/<pid>/fd/2 -> /dev/null
```

## The cause

Three sites spawned processes with `cmd.Stderr = nil`, which sends stderr to nowhere:

- `internal/daemonctl/daemonctl.go:56` — `EnsureRunning`, the autostart called by **every**
  CLI invocation (`cmd/graphit/commands/root.go:43`)
- `cmd/graphit/commands/daemon.go:121` — `spawnDetachedDaemon`, the daemon replacement
- `cmd/graphit/commands/lifecycle.go:1002` — the `sync --heavy` spawn, which is not the daemon

The point that makes this matter: **panic and `SIGQUIT` dumps are written by the runtime to stderr,
not via the logger.** The supervisor logs what it chose to log — module lifecycle,
supervised module crash — and nothing else. So a deadlock on the main goroutine, a panic
outside a module, or any stall of the daemon process left no trace at all, and from outside "stuck"
and "idle" are indistinguishable.

## The fix

`AttachStderrToFile(cmd, path)` opens the file in append, points the child process's stderr to it
and returns the parent's copy closer — the child inherits the descriptor, so the parent closes after
`Start`. `AttachLogStderr(cmd)` is that pointed at `daemon.log`. Both daemon sites now
use it, and `internal/daemon` re-exports both for the `commands` package, which is the importer.

**Best effort on purpose:** if the file won't open, the process is spawned anyway. Losing
the daemon is worse than losing its stderr.

## Tests

`internal/daemonctl/stderr_log_test.go`:

- `TestAttachStderrToFileAppendsChildStderr` — test binary re-executes itself as a child,
  writes to stderr something shaped like a stack dump, and the test requires that to appear in
  `daemon.log` **and** that the pre-existing line survives. Proves descriptor inheritance and
  append, which a unit test on the helper wouldn't catch.
- `TestAttachStderrToFileIsBestEffort` — with the log impossible to open, stderr stays
  unassigned and the process still comes up.
- `TestLogFilePathIsInTheDaemonDir` — the path.

`gofmt` clean, `go vet` clean on both packages, `internal/daemonctl` and `internal/daemon` green,
`go build ./cmd/graphit/...` ok.

## What remains open

- **The third site.** `lifecycle.go:1002` spawns `sync --heavy`, whose errors already go to
  `sync.log` — its stderr belongs in the same file, not `daemon.log`. Tried together, the
  edit didn't match, and I preferred not to ship half a change: left for a second step.
- **Why that daemon got stuck.** The stack was lost with `SIGQUIT` on that process. With this
  fix, the next occurrence remains readable — which is the goal here, not the cause.
- **Mutual exclusion between CLI and daemon** indexing the same project. Autostart on every invocation
  is intentional, and it's a guarantee of a live daemon; contention for the same graph is another matter.
