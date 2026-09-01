---
title: No memory was committed because the daemon inherited the environment from a Git hook.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [git, daemon, memoria, bugfix, ambiente]
---

A relative that is inherited from a hook, stopped within the daemon

Origin: It appeared during the first recording of the session that corrected the index on the wiki. Every `graphit_memory_insert` responded:

```
committing memory: staging memory changes: exit status 128:
fatal: Unable to create '<worktree>/.git/index.lock': Not a directory
```

It wasn't new either: there were 17 and 18 of August's memos stuck as __INLINE_2__ in the memory worktree. They were written to disk and never committed — the search was still working, because the wiki is compiled from files rather than Git, so the symptom only appears when someone looks at __INLINE_3__ in the memory worktree.

---

## O defeito

The daemon had this in its environment:

```
GIT_INDEX_FILE=.git/index
GIT_PREFIX=
GIT_AUTHOR_DATE=@1787003188 -0300
GIT_EXEC_PATH=/usr/lib/git-core
```

It is the set that a **git hook exports**. A hook that initiates a long-lived process delivers it to this environment forever — and the daemon re-executes itself upon detecting version changes, passing on its own environment ahead, so pollution traverses every restart since when the hook initiated it.

INLINE 4 is relative, and that's the part it bites: it re-resolves against the target of each INLINE 5. In a linked worktree — which is what the memory-based worktree is — INLINE 6 is an **file** with a pointer INLINE 7, not a directory. Creating INLINE 8 gives INLINE 9.

Reproduced in one line:

```bash
env GIT_INDEX_FILE=.git/index git -C <worktree> add .
# fatal: Unable to create '<worktree>/.git/index.lock': Not a directory
```

Why did it cost to find

The message names a `index.lock`_. This reads as containment — two processes competing for the repository — and prompts for competition, daemon, race. There is no lock; the file could never be created because the path passes through an ordinary file.

And it was running perfectly from the shell, which rules out permissions, corrupted worktrees, and repository state—just the process environment that called.

Correction

Inline 12: Inline 13 passed to filter the inherited environment with
Inline 14, which removes variables that Git describes as "the current invocation":

`GIT_DIR`, `GIT_COMMON_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_INDEX_VERSION`,
`GIT_PREFIX`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_NAMESPACE`,
`GIT_GRAFT_FILE`, `GIT_QUARANTINE_PATH`, `GIT_REFLOG_ACTION`, `GIT_AUTHOR_DATE`,
`GIT_COMMITTER_DATE`.

All calls to this package name the repository with `-C`, so inheriting any of them is always wrong:
They point Git back to another repository, index, or moment that belongs to something else.

The dates are listed on the list for a different reason than the same cause: inherited, they stamp all commits with the hook's time instead of the moment when the work was done.

The author's identity is not removed. `GIT_AUTHOR_NAME` and `EMAIL` are part of the same family, but Git falls into the same configuration when they are missing, and these are the only ones in that family that a legitimate person defines for an entire session. Removing them would silently alter the authorship.

The filter operates only on the **inherited environment**. `buildCmd` appends the `env` explicit of the caller afterward, and the subsequent assignment takes precedence, so anyone who intends to define one of them purposefully can continue doing so. Nothing in the code does this today — checked before writing the filter.

## Testes

`internal/git/hook_env_test.go`:

- **INLINE_35** - sets up a repository and a worktree that is actually connected (confirming that the `.git` of it is an actual file, because that's where the defect lives), passes the environment via `t.Setenv`, and asserts three things: the `git` cru still fails with this environment — if it stops failing, nothing more is caught by the test —, the same command from this package works, and the resulting commit **does not** inherit `GIT_AUTHOR_DATE`.
- **INLINE_40** - checks the other side: `PATH`, `HOME`, `GIT_SSH_COMMAND`, and `GIT_AUTHOR_NAME` survive the filter.

## Arquivos alterados

| File | Change | Reason |
|---|---|---|
| `internal/git/cli_backend.go` | Modified | _Inline_46__ in `buildCmd` |
| `internal/git/hook_env_test.go` | Created | Regression with linked worktree and hook environment |


## Conhecimento do sistema

After __LINE__ 49__, the error continued— and that doesn't mean the correction didn't work.  
The MCP server for an agent session is an old process with the binary already replaced on disk:

```
$ readlink /proc/3835364/exe
~/.graphit/runtime/dev/graphit-mcp (deleted)
```

Who ran the new binary was the **daemon**, which restarts itself automatically when it detects a version change — and the daemon is what executes Git's memory operations, which fixed the insert to work without touching the session. Before concluding that a fix didn't take, check `readlink /proc/<pid>/exe`: `(deleted)` is the answer.


The daemon's log records the race between the two things, and it is benign:


```
Error replacing process with core binary: text file busy
```

The daemon is attempting to restart itself while `make install` is still writing the file. It manages to do so on the next attempt.

Technical Debt

This term refers to the accumulated technical debt in software development projects. It represents a backlog of unfinished or poorly implemented features that have not been addressed due to time constraints or other factors. The goal is to identify and mitigate these issues before they become critical problems, ensuring the project remains maintainable and adaptable over time.

Debt of expertise


- [ ] The daemon can still be started via a hook and inherit its environment. The filter resolves the consequence — no Git command from this package is affected — but the process continues to load variables that are not its own, and any future code that calls `git` without passing through `internal/git` returns to having the problem. The root fix is to sanitize the daemon's environment when spawning it.
