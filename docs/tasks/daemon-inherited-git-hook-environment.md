---
title: No memory was being committed because the daemon inherited the environment of a git hook
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [git, daemon, memoria, bugfix, ambiente]
---

# A relative `GIT_INDEX_FILE`, inherited from a hook, stuck inside the daemon

**Origin:** it showed up when writing the first memory of the session that fixed the wiki index.
Every `graphit_memory_insert` answered:

```
committing memory: staging memory changes: exit status 128:
fatal: Unable to create '<worktree>/.git/index.lock': Not a directory
```

And it was not new: there were memories from August 17 and 18 sitting as `untracked` in the memory
worktree. **They were being written to disk and never committed** — search kept working,
because the wiki is compiled from the files and not from git, so the symptom only appears when someone
looks at the worktree's `git status`.

---

## The defect

The daemon had this in its environment:

```
GIT_INDEX_FILE=.git/index
GIT_PREFIX=
GIT_AUTHOR_DATE=@1787003188 -0300
GIT_EXEC_PATH=/usr/lib/git-core
```

This is the set a **git hook exports**. A hook that starts a long-lived process hands
it that environment forever — and the daemon re-execs itself when it detects a version change, passing
its own environment along, so the pollution went through every restart since the day the hook
started it.

`GIT_INDEX_FILE=.git/index` is **relative**, and that is the part that bites: it re-resolves against the
target of each `git -C`. In a linked worktree — which is what the memory worktree is — the `.git` is
a **file** with a `gitdir:` pointer, not a directory. Creating `<worktree>/.git/index.lock`
gives `ENOTDIR`.

Reproduced in one line:

```bash
env GIT_INDEX_FILE=.git/index git -C <worktree> add .
# fatal: Unable to create '<worktree>/.git/index.lock': Not a directory
```

## Why it was expensive to find

**The message names an `index.lock`.** That reads as contention — two processes fighting over the
repository — and sends you looking for concurrency, daemon, race. There is no lock at all: the file could never
be created, because the path goes through the inside of an ordinary file.

And `git -C <worktree> add .` run from the shell worked perfectly, which rules out
permissions, a corrupted worktree and repository state — leaving only the environment of the process
that was calling.

## The fix

`internal/git/cli_backend.go`: `buildCmd` now filters the inherited environment with
`withoutInheritedGitScope`, which removes the variables git uses to describe **the invocation in
progress**:

`GIT_DIR`, `GIT_COMMON_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_INDEX_VERSION`,
`GIT_PREFIX`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_NAMESPACE`,
`GIT_GRAFT_FILE`, `GIT_QUARANTINE_PATH`, `GIT_REFLOG_ACTION`, `GIT_AUTHOR_DATE`,
`GIT_COMMITTER_DATE`.

Every call in this package names the repository with `-C`, so inheriting any of them is
always wrong: they re-point git at a repository, an index or a moment that
belong to something else.

**The dates are on the list because of a different failure with the same cause**: inherited, they stamp every
commit with the hook's time instead of the moment the work happened.

**The author identity is NOT removed.** `GIT_AUTHOR_NAME`/`EMAIL` are from the same family, but
git falls back to the same config when they are missing, and they are the only ones in that family that a person
legitimately sets for a whole session. Removing them would change authorship silently.

The filter acts only on the **inherited** environment. `buildCmd` appends the caller's explicit `env`
afterwards, and the later assignment wins, so anyone who wants to set one of them on purpose
still can. Nothing in the code does that today — checked before writing the filter.

## Tests

`internal/git/hook_env_test.go`:

- `TestGitCommandsIgnoreAnInheritedHookEnvironment` — sets up a repository and a genuinely
  **linked** worktree (checking that its `.git` is a file, because that is where the defect
  lives), puts the hook environment in via `t.Setenv`, and asserts three things: raw `git` still fails
  with that environment — if it stops failing, the test no longer pins anything and says so —, the same
  command through this package passes, and the resulting commit does **not** inherit `GIT_AUTHOR_DATE`.
- `TestWithoutInheritedGitScopeKeepsEverythingElse` — checks the other side: `PATH`, `HOME`,
  `GIT_SSH_COMMAND` and `GIT_AUTHOR_NAME` survive the filter.

## Changed files

| File | Change | Reason |
|---|---|---|
| `internal/git/cli_backend.go` | Modified | `withoutInheritedGitScope` in `buildCmd` |
| `internal/git/hook_env_test.go` | Created | Regression with a linked worktree and a hook environment |

## System knowledge

**After `make install`, the error continued — and that does not mean the fix did not take.**
The MCP server of an agent session is an old process, with the binary already replaced on
disk:

```
$ readlink /proc/3835364/exe
~/.graphit/runtime/dev/graphit-mcp (deleted)
```

The one that started running the new binary was the **daemon**, which restarts itself when it sees the version
change — and the daemon is what runs the git of the memory operations, which is what made the insert
work again without touching the session. Before concluding that a fix did not take, check
`readlink /proc/<pid>/exe`: `(deleted)` is the answer.

The daemon log records the race between the two things, and it is benign:

```
Error replacing process with core binary: text file busy
```

— the daemon trying to re-exec while `make install` was still writing the file. It
succeeds on the next attempt.

## Technical debt

- [ ] **The daemon can still be started by a hook and inherit its environment.** The filter
  fixes the consequence — no git command in this package is affected any more — but the process
  keeps carrying variables that are not its own, and any future code that calls `git` without
  going through `internal/git` gets the problem back. The fix at the root is to sanitize the environment when
  spawning the daemon.
