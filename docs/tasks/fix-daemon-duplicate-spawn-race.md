# Fix: Daemon Duplicate Spawn Race Condition

**Date:** 2026-06-24
**Status:** Done

## Problem

Two daemon processes (`graphit-core daemon`) were running simultaneously, observed
via `ps aux`. Both had the same PID stamp (`b1551811...`) and started within 1
second of each other (23:08:40 and 23:08:41). Only one (PID 2937834) held the
PID file lock; the other (PID 2937799) was a zombie with no lock, consuming
~500 MB RAM and 2–4% CPU without any legitimate ownership.

## Root Cause — TOCTOU Race in `EnsureRunning`

Three `graphit-mcp` processes (2553714, 2556648, 2557927) were running. When the
previous daemon stopped, the `mcpproxy` reconnect loop called `EnsureDaemon()`
every 500 ms. Multiple callers raced through `isDaemonLocked()`:

```
isDaemonLocked():
  1. os.Open(PID file)   — fails if file missing → returns false
  2. flockProbe (LOCK_EX|LOCK_NB) — succeeds → releases lock → returns false
```

Between step 2 (lock released) and the daemon actually acquiring its PID lock,
all concurrent callers see "not locked" and each call `cmd.Start()` to spawn
a new daemon. This is a classic TOCTOU (Time-of-check Time-of-use) race.

## Fix

Added a **blocking spawn-lock** (`~/.graphit/daemon/.spawn.lock`) in
`internal/daemonctl/daemonctl.go`. `EnsureRunning` now:

1. Opens `.spawn.lock` with `O_CREATE|O_RDWR`
2. Acquires a **blocking** `LOCK_EX` (without `LOCK_NB`) — concurrent callers
   queue up here instead of racing past
3. Re-checks `isDaemonLocked()` under the lock — if a peer already spawned the
   daemon and it acquired its PID lock, this returns true and we skip the spawn
4. Spawns daemon only if still not running
5. Releases spawn-lock via `defer` — the next queued caller wakes up, sees the
   daemon is now locked, and skips

The daemon's own `Acquire()` flock in `pidfile.go` remains as a final guard,
but the spawn-lock prevents the window where multiple daemon processes are
created before any of them writes the PID file.

## Files Changed

- `internal/daemonctl/flock_unix.go` — added `flockExclusiveBlocking` (LOCK_EX without LOCK_NB)
- `internal/daemonctl/flock_windows.go` — added `flockExclusiveBlocking` (LockFileEx without LOCKFILE_FAIL_IMMEDIATELY)
- `internal/daemonctl/daemonctl.go` — rewrote `EnsureRunning` with spawn-lock

## Immediate Mitigation

Killed zombie PID 2937799 with `SIGTERM`. The legitimate daemon (2937834) was
unaffected and continued running. The deleted PID file (removed by 2937799's
`Release()` on shutdown) is a cosmetic issue — the daemon holds the flock even
without the file on disk.
