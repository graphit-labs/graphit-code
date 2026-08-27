The lint was not INLINE_0__ of Git — the tests were talking to the real remote.

The diagnosis of the backlog was incorrect.

O item registrava a falha

```
TempDir RemoveAll cleanup: unlinkat …/repo/.git: directory not empty
```

as race between removing the `t.TempDir()` and a maintenance process left behind by
The suggested correction was to disable automatic maintenance. This is true and...
entered - but it wasn't the cause.

## A causa

The test read from `HOME`.
Configure the global real memory of the machine, with `memory.repo` pointing to a private remote.
Truth. Therefore:

The `INLINE_0` adds this `INLINE_1` to the temporary test repository.
The second function, INLINE_0, calls INLINE_1, which stops when it fails.
The _INLINE_0_ is empty - it wasn't.
3. Uma **goroutine** passa a rodar `syncRemote()`, `rebaseOntoTrackingRef` e
   `push --set-upstream origin <branch>` dentro do worktree, viva depois de o teste
   retornar.
The removal of INLINE_0__ is handled by this writer.

This also explains what the backlog didn't explain: the error target **changed** — INLINE 0
In a run, `.git/worktrees/<nome>` elsewhere— because it depends on which Git command
estava em voo quando a limpeza rodou.

And there's the side that isn't about flakes: the test was trying to push test branches into ```
Repository of Real Developer Memory

The measurement that proves

Isolando `HOME`, o pacote `internal/memory` caiu de **18,4s para 0,98s**. Dezessete
segundos eram rede contra o remote real.

It serves as a general heuristic: when a testing package takes much longer than expected.
The work he does warrants suspicion of inherited configuration before suspecting the code.

Correction

Em `TestMain` de `internal/memory` e `internal/hub`:

"`HOME` and `USERPROFILE` into a temporary directory." `MemoryRepoURL()` becomes empty.
The goroutine never starts - it's the guard that the very `pushBranchInBackground` already
   checava.
Identity Git by Environment Variable. Isolate `HOME` to hide `~/.gitconfig`;
   o git recusa commitar sem identidade; apareceu como `unable to auto-detect email
Exporting it is the right thing to do, no matter what — a test that commits should not
   depender de quem o executa.
3. **`git.DisableAutoMaintenance()`**, em `internal/git/maintenance.go`, exportando
   `gc.auto=0` e `maintenance.auto=false` via `GIT_CONFIG_*`. Fica como defesa em
Depth: A commit triggers `gc --auto`, which is an independent second race of its own.
First, append to the existing exported configuration instead of overwriting it, and there's a test for that.
Question Git itself if the configuration arrived—just stating that we set it wasn't enough.
variable
4. **`WaitForPendingPushes()`** depois de `m.Run()`.

``TestMain``, and not one line in each helper, because the repositories that import are not
All created by Helper: A store creates its own worktrees and clones, and each one is a separate instance.
repository that Git would maintain on its own.

## Armadilha encontrada no caminho

Before `os.Exit`, it doesn't work — Gocritic caught me. Clean up the `HOME` temporary with
The inline 0 would create an execution directory. In inline 1, cleanup is explicit.

Verification

Complete Suite x Cleaned. **Inline 0** + **Inline 1** with **Inline 2** under Load of Two Suites
Complete in parallel: Clean. Since the defect is probabilistic, that's not a proof —
but it's the same condition as before that was reproduced, and now the cause has been removed instead of
mitigada.

## Progress Log

August 16, 2026 - Root cause identified and corrected; three new tests conducted.
  `internal/git` (o git confirma a config, e a config existente sobrevive ao append).
Complete suite green, **0 issues**.
