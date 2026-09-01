---
title: Tests now run in an ephemeral HOME — and this revealed 6 tests that depended on the machine
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [tests, isolation, brand, ast, hermeticity]
---

# Tests now run in an ephemeral HOME — and this revealed 6 tests that depended on the machine

## Objective

The Engineer observed that running the suite polluted the machine's **real** `~/.graphit`, with
projects and memories from temporary directories. Every test should run in an ephemeral
directory.

The observation was correct, and the measurement was worse than the suspicion.

## What was measured before touching anything

In this machine's real `~/.graphit`:

| Residue | Amount | Proven origin |
|---|---|---|
| `ast/project/path-*` | 43 directories, 87 MB | manifests with `existing.sql`, `a.sql`, `b.sql`, `criada.sql` |
| `wiki/knowledge/project/path-*` | 39 directories, 73 MB | manifests with `docs/test.md` and `README.md` |
| `memory.lock.json` | 2 orphan branches | `memory/project/test-proj` pointing to 4 worktrees in `/tmp/TestMemoryService_*` already deleted, and `memory/project/validate-test` |

**160 MB**, and none of the 82 `path-*` directories corresponded to a real project — the
classification was done by reading each one's manifest, not by sampling. The most recent ones
had a mtime from the current day, so the pollution was ongoing, not historical.

`path-<hash>` is the key of a project **without a lockfile** (`store.pathStoreID`: `sha256` of
the first 16 hex characters of the absolute path). A real project has a ULID — on this machine
there are only two, graphit-code and the private corpus. Every `path-*` was a
temporary test directory.

Isolation coverage before: **17 of 44 packages** touched `HOME` in some test, with 6 different
helpers under 3 names (`withHome`, `isolateHome`, `testHome`), and only 3 packages with
`TestMain`. `internal/ast` isolated in 4 of 139 test files.

## The fix: a single point, and it's an environment variable

`brand.GlobalDir()` is `os.UserHomeDir() + "/.graphit"`, and `os.UserHomeDir()` reads `$HOME`.
An `init()` in `internal/brand/testhome.go` points `HOME` to a disposable directory when
`testing.Testing()` is true.

**Why an environment variable and not a guard inside `GlobalDir()`:** `GlobalDir()` is not the
only path to the operator's home. `os.UserHomeDir()` is called directly in 9 other places —
`internal/config` (the global config), `internal/ai` (the 132 MB model cache),
`internal/hub/adapters/ide`, `cmd/launcher` — and none of them go through `GlobalDir()`. Moving
`HOME` covers all of them at once, including any added later.

And it covers two cases that no in-process check can reach:

1. **Subprocess.** A test that spins up the daemon hands it this process's environment. The
   child is not a test binary, so `testing.Testing()` is false there, and it would resolve the
   real home regardless of what this package returned here.
2. **Git's own config.** A temporary repository reads `~/.gitconfig`, and on a real
   installation that config names a memory remote — which is how a test run ends up adding a
   live repository as `origin` and pushing test branches to it (see
   the earlier cleanup-flake investigation, which established that the tests were talking to the real remote).
   `XDG_CONFIG_HOME` follows `HOME` because git prefers `$XDG_CONFIG_HOME/git/config`.

A package that isolates `HOME` on its own still benefits: the `init()` runs before any
`TestMain` and any `t.Setenv`, so it's the floor, never the ceiling.

### The cost of importing `testing` into production code: measured

15,625 bytes — 265,379,408 → 265,395,408, **0.006%** of the binary. The heavy dependencies of
`testing` (`flag`, `regexp`, `runtime/pprof`) were already linked in through other paths. The
trade-off that seemed to need discussion doesn't exist.

## What the change revealed: 6 tests green by accident

The 6 passed **only** because they read the developer's real `~/.graphit`. All were confirmed
green on the baseline (via `git stash`) and red with isolation — meaning they weren't
pre-existing failures, they were hidden dependencies that isolation exposed.

| Test | Package | Real cause |
|---|---|---|
| `TestEmbeddedLangResolvesAcrossBothBackends` | ast | no language resolves without the installed queries |
| `TestParsePoolResetsAntlrCachesWithParsesInFlight` | ast | `unknown ANTLR grammar: antlr-plsql` |
| `TestParquetRoundTripPreservesGraph` | ast | empty graph because nothing parsed |
| `TestGraphSamplesRunOnAGraphWithoutEntities` | ast | same |
| `TestPrepareASTPublishPrefersParquet` | hub | publishing an AST artifact builds a real graph |
| `TestDaemonLeavesTheDirectoryItWasSpawnedFrom` | daemon | the assertion used `os.TempDir()` as a proxy for "ephemeral" |

### The seventh, which only showed up in the full suite: git's identity

`TestGitCLIBackend` (`internal/git`) failed with

```
git commit failed: exit status 128: fatal: unable to auto-detect email address
(got '<user>@<host>.(none)')
```

Emptying `HOME` is exactly what hides `~/.gitconfig`, and git refuses to commit without an
identity. `internal/memory` and `internal/hub` already handled this in their own `TestMain` —
they export `GIT_AUTHOR_*`/`GIT_COMMITTER_*` for exactly this reason. With isolation applying
to all 44 packages, identity became a direct consequence of the `init()` and was resolved
there, rather than left for every committing package to rediscover.

This only showed up in the full `make test`, not in the 12 packages I ran targeting the
residue — which is the argument for running the whole suite before declaring it done, not just
the packages the change seemed to touch.

### Language definitions are not compiled into the binary

The chain, worth recording because it isn't obvious:

```
internal/ast/queries/*.yaml
  → cmd/launcher/runtime/ast/queries/   (cp in the Makefile, line 289)
  → go:embed in the launcher             (cmd/launcher/embed.go)
  → extracted at install time to ~/.graphit/runtime/<version>/ast/queries/
```

`rebuildExtTables` reads **only** the final destination. The test binary never had an
installer, so it saw zero languages. Before isolation it found the runtime of whichever
version the developer had installed — which made the green suite depend on the machine: a
contributor who never ran the installer, and CI, would see failures that don't reproduce
anywhere else.

The fix is `internal/testsupport`, which seeds the queries **from this checkout** into the
isolated runtime dir. It exercises the production load path instead of bypassing it, so the
loader, merge order, and extension tables remain under test.

**The ordering is the detail that makes it work.** `internal/ast` builds its tables in its own
`init()` (`treesitter_adapter.go:300`), reading the directory once and caching the result —
including when empty. `TestMain` runs **after** all `init()` functions, so seeding there would
be too late. The seeding lives in `testsupport`'s `init()`: a package's dependencies are
initialized before it is, so importing `testsupport` from `internal/ast`'s test files puts that
`init()` ahead of `internal/ast`'s — and `internal/brand`'s, a dependency of `testsupport`,
ahead of both.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/brand/testhome.go` | Created | the `init()` that points `HOME`/`USERPROFILE`/`XDG_CONFIG_HOME` to a disposable directory, and exports the git identity that emptying `HOME` just hid |
| `internal/brand/testhome_test.go` | Created | regression test: a test binary must resolve the home into the ephemeral root |
| `internal/testsupport/runtimequeries.go` | Created | seeds the checkout's grammar queries into the isolated runtime dir, in `init()` |
| `internal/ast/main_test.go` | Created | fails loudly and early if the seeding didn't happen |
| `internal/hub/main_test.go` | Modified | same check; publishing an AST artifact builds a real graph |
| `internal/daemon/daemon_cwd_test.go` | Modified | the "not under `os.TempDir()`" assertion started contradicting the assertion above it |
| `internal/brand/brand_test.go` | Modified | `TestGlobalDirsAndResolvers` leaked `Brand = "testbrand2"` into every following test |
| `Makefile` | Modified | sweeps homes from the previous run in the `test` target |

## Trade-offs & Decisions

**`init()` touching `HOME` versus a guard in `GlobalDir()`.** Chose the `init()`: it covers the
9 direct callers of `os.UserHomeDir()`, the subprocesses, and git. The guard would only cover
callers going through `GlobalDir()`.

**Silent redirect versus panicking.** The redirect is silent because the goal is the outcome —
no test touching the real home — and a panic would break 44 packages at once without anyone
having asked for that. But when the temporary directory **cannot be created**, that does
panic: a silent fallback on that path is exactly the bug this file exists to remove, and a
non-writable `/tmp` already breaks `t.TempDir()` and nearly the whole suite.

**Copy instead of symlink for the queries.** A symlink would leave the repository as the
single copy and would pick up mid-session edits — no value here, since every test binary gets
a fresh home and reseeds regardless — while it would turn any write into that directory into a
silent edit of this checkout's versioned files. Today nobody writes there; copying means
nothing has to keep being true for this to stay safe. It's 45 small files.

**The sweep runs before the run, not after.** `os.Exit` doesn't run `defer`, and a package
without its own `TestMain` offers no hook after `m.Run()` — so nothing can delete the home on
exit. Sweeping beforehand caps the residue at roughly one run's worth and doesn't rip the
`HOME` out from under a process that a leaked test left running, which sweeping afterward
would do.

## Technical Debt

- [ ] Ephemeral homes are not removed at the end of the process — only swept by `make test` on
      the next run. Anyone running `go test ./...` directly accumulates one directory per
      binary in `/tmp`. A full run produced **323 MB** in `internal/ast` + `internal/daemon`
      alone.
- [ ] The 27 packages that still didn't isolate `HOME` now depend on `brand`'s floor. That's
      intentional, but it means none of them declares its own need for it — if the `init()`
      is removed, the pollution comes back silently. `internal/brand/testhome_test.go` is the
      only thing guarding that door.
- [ ] 8 test files in `internal/daemon` use `os.Setenv("HOME", …)` with `defer` instead of
      `t.Setenv`, which is incompatible with `t.Parallel()` and leaks if the test fails before
      the defer runs. With `brand`'s floor in place, most of them can simply be deleted.
- [x] The **82 `path-*` directories** (160 MB) have already been removed from the Engineer's
      `~/.graphit` — `find ~/.graphit -maxdepth 3 -name 'path-*'` returns **0**, and only the
      two real-project ULID keys remain, graphit-code's and the private corpus's.
- [ ] The **2 orphan branches in `memory.lock.json` are still there** — this item was closed
      by mistake in an earlier revision of this section, which checked only the `path-*`
      directories and assumed the rest. `memory/project/test-proj` still references 4 deleted
      worktrees in `/tmp/TestMemoryService_*`, and `memory/project/validate-test` has
      `refs: ["user"]`. These are leftovers from testing prior to isolation; isolation
      prevents new ones, it doesn't clean up the old ones. Removal is left to the Engineer,
      since it's the real global lockfile.
- [ ] `internal/memory/main_test.go` and `internal/hub/main_test.go` still export the git
      identity that `brand`'s `init()` now exports for everyone. It's harmless duplication
      (identical values) and was left in on purpose, because those files carry the reasoning
      for the rest of what they isolate — but it is duplication.

## System Knowledge

- **`brand.GlobalDir()` is `$HOME/.<brand>` and nothing else** — no override via environment
  variable. `HOME` is the only control point, which is exactly why the fix is an environment
  variable.
- **`testing.Testing()` works inside `init()`.** It's a value the linker sets when building a
  test binary, so it's already correct before any `init()` runs.
- **`make test` no longer passes `-tags fts5`.** Commit `fb19403` removed SQLite from the
  binary; only a historical mention remains in a Makefile comment (lines 541-542). The memory
  that said otherwise was corrected in this session.
- **`Brand` is a mutable global that tests rewrite.** Anything that derives a path from `Brand`
  at call time is unstable within the suite; `internal/brand/testhome.go` stores the created
  home in a variable instead of recomputing the root.
- **The `daemon_cwd_test.go` assertion was a proxy that aged poorly.** "Not under
  `os.TempDir()`" meant "not in a directory someone is going to delete." With the global dir
  living under `/tmp` during tests, the proxy started contradicting the assertion immediately
  above it, which requires `cwd == brand.GlobalDir()`.

## Progress Log

### 2026-08-18
- Measured the real residue: 160 MB, 82 directories, all classified by manifest.
- `internal/brand/testhome.go`: ephemeral `HOME` per test binary, under a single parent.
- Measured the cost of importing `testing` in production: 15 KiB, 0.006%.
- Confirmed with a `git stash` baseline that the 6 failures were hidden dependencies on the
  developer's home, not regressions.
- `internal/testsupport`: query seeding in `init()`, following initialization order.
- Empirical check: real `~/.graphit` byte-for-byte identical after 12 packages, including the
  ones that produced all the original residue.

### 2026-08-18 (final verification, following session)

The previous session ended with the full suite in flight and no result recorded. Run and
verified here.

**Full `make test`: 44 packages `ok`, zero `FAIL`, zero build errors.** No package is left
out: `GO_PKGS_SKIP` only separates the two passes (`/antlr/|/treesitter/` is excluded from the
`-race` pass), and pass 2 runs exactly the packages pass 1 excluded. The ones the change
touched: `internal/ast` 195.6 s (66.1%), `internal/daemon` 52.1 s (79.4%), `internal/brand`
1.0 s (95.5%), `internal/git` 1.1 s (99.2%), `internal/hub` 3.1 s (53.5%).

**The real home was not touched.** Snapshot of `ast/`, `wiki/`, `memory/`, `memory-wt/` up to
`maxdepth 3` before and after the run: empty `diff`. `memory.lock.json` and `global.lock.json`
identical. Zero new `path-*`.

**Where the residue went.** `/tmp/graphit-test-homes/` with 105 homes and 328 MB, each
containing exactly what used to go to the real home:
`home-*/.graphit/{ast/project/path-*,wiki/knowledge/project,models/coderankembed,memory-wt}` —
plus `home-*/.lbdb/extension/`, which is LadybugDB's extension and also used to come out of
`$HOME`. The `make test` sweep worked: 112 homes / 330 MB from the previous run were deleted
before this run started.

**Git's identity is the fake one, and `~/.gitconfig` is unreachable.** Measured with a
temporary probe in `internal/brand` that did `git init` + `commit` in a `t.TempDir()` (removed
afterward):

| what | value inside the test binary |
|---|---|
| `HOME` | `/tmp/graphit-test-homes/home-3470831534` |
| `XDG_CONFIG_HOME` | `…/home-3470831534/.config` |
| commit author and committer | `Test <test@example.com>` |
| `git config --get user.email` | `""` — empty |
| `git remote -v` | `""` — nothing inherited |

The empty `user.email` line settles both requirements at once: git resolves identity through
the `GIT_AUTHOR_*`/`GIT_COMMITTER_*` variables, and the developer's global config — whatever
`user.name`/`user.email` is on the machine — simply doesn't exist from there. This is also why
no `memory.repo` can be inherited, which is the mechanism behind the accidental push described
in `docs/tasks/` and in the corresponding memory.

`golangci-lint` on the five touched packages: **0 issues**.

**The `/tmp` debt is measured, and it's smaller than it looked:** `/tmp` here is `tmpfs`
(31 GB, 33% used) and `systemd-tmpfiles` cleans entries after 10 days. So the accumulation
from anyone running `go test ./...` directly has two independent ceilings beyond the
`make test` sweep.
