The bug was not really about Git isolation — the tests were talking to the real remote.

The diagnosis in the backlog was incorrect.

The backlog item recorded the failure

```
TempDir RemoveAll cleanup: unlinkat …/repo/.git: directory not empty
```

as a race between removing the `t.TempDir()` and a maintenance process left behind by Git.
The suggested fix was to disable automatic maintenance. That is true, and it did play a
part — but it wasn't the root cause.

## The Cause

The test read from `HOME`, which pointed to the machine's real global Git config, with
`memory.repo` pointing to a private remote. True. Therefore:

1. The test setup reads `memory.repo` from the global config and adds it as a remote on
   the temporary test repository.
2. The second function, `pushBranchInBackground`, calls `syncRemote`, which only stops
   early if `MemoryRepoURL()` is empty — it wasn't.
3. A **goroutine** starts running `syncRemote()`, `rebaseOntoTrackingRef`, and
   `push --set-upstream origin <branch>` inside the worktree, and stays alive after the
   test has already returned.

That same goroutine is still touching those files when cleanup runs.

This also explains what the backlog didn't explain: the error target **changed** between
runs — sometimes `.git/worktrees/<name>`, sometimes somewhere else — because it depends
on which Git command was in flight when the cleanup ran.

And there's a side to this that isn't about flakiness at all: the test was trying to push
test branches into the developer's real memory repository.

The measurement confirms it: isolating `HOME`, the `internal/memory` package dropped from
**18.4s to 0.98s**. Seventeen of those seconds were network time spent against the real
remote.

This serves as a general heuristic: when a test package takes far longer than expected,
suspect inherited configuration before suspecting the code.

## Correction

In `TestMain` of `internal/memory` and `internal/hub`:

1. Point `HOME` and `USERPROFILE` to a temporary directory: `MemoryRepoURL()` becomes
   empty, so the goroutine never starts — it's the same guard that `pushBranchInBackground`
   was already checking for.
2. Set the Git identity via environment variable. Isolating `HOME` also hides
   `~/.gitconfig`, so Git refuses to commit without an identity; it surfaced as
   `unable to auto-detect email address`. Exporting the identity explicitly is the right
   thing to do regardless — a test that commits should not depend on who runs it.
3. **`git.DisableAutoMaintenance()`**, in `internal/git/maintenance.go`, exports
   `gc.auto=0` and `maintenance.auto=false` via `GIT_CONFIG_*`. This stays in as defense
   in depth: a commit triggers `gc --auto`, which is an independent race of its own.
   Important: append to the existing exported configuration instead of overwriting it —
   there's a test for that. Ask Git itself whether the configuration took effect; just
   asserting that we set the variable wasn't enough.
4. **`WaitForPendingPushes()`** after `m.Run()`.

All of this lives in `TestMain`, not repeated per helper, because the repositories
involved aren't all created by a single helper: a store creates its own worktrees and
clones, and each one is a separate repository that Git would otherwise maintain on its
own.

## Pitfall found along the way

A `defer` does not run before `os.Exit` — `gocritic` caught this. The temporary `HOME`
directory has to be cleaned up explicitly instead: one step creates the directory, and a
separate, explicit step performs the cleanup.

## Verification

Full suite, clean. Running under the race detector, repeated runs, and two full suites
in parallel: clean. Since the defect is probabilistic, that's not a proof — but it
reproduces the same condition as before, and now the cause has been removed instead of
merely mitigated.

## Progress Log

**August 16, 2026** — Root cause identified and fixed; three new tests added, covering
`internal/memory`, `internal/hub`, and `internal/git` (verifying that Git confirms the
configuration, and that existing configuration survives the append). Full suite green,
**0 issues**.
