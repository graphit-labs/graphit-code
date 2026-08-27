---
The tests started running on an ephemeral HOME — and this revealed 6 tests that depended on the machine.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [testes, isolamento, brand, ast, hermeticidade]
---

The tests started running in an ephemeral home — and this revealed 6 tests that depended on the machine.

## Objetivo

The Engineer observed that running the suite polluted the **actual** core of the machine, with
Projects and temporary directory memories. All tests should run in a temporary directory.
ephemeral

The observation was correct, and the measurement exceeded the suspicion.

## O que foi medido antes de mexer em nada

In this actual machine:

Residue | Quantity | Verified Origin
|---|---|---|
The Portuguese text translates to:

"_`ast/project/path-*`_ | 43 directories, 87 MB | manifests with _`existing.sql`_, _`a.sql`_, _`b.sql`_, _`criada.sql`_"

This is already in idiomatic English. No translation needed.
Here is the translation:

"_`wiki/knowledge/project/path-*`_ | 39 directories, 73 MB | manifests with `docs/test.md` and `README.md`"
The Portuguese text is already in idiomatic English. Here's the translation:

| `memory.lock.json` | 2 orphan branches | `memory/project/test-proj` pointing to 4 worktrees that have been deleted at `/tmp/TestMemoryService_*`, and `memory/project/validate-test` |

This sentence appears to be describing a situation where there are two orphaned branches, four deleted worktrees, and an unspecified third element (denoted by `memory/project/validate-test`). The structure is similar in both languages, making the translation straightforward.

**160 MB**, and none of the 82 directories corresponded to an actual project— in **INLINE_0**.
The classification was made by reading each person's manifesto, not through sampling. The latest
They had their own time of day, so pollution was common, not historic.

The ``path-<hash>`` is the key of a project without a lockfile (``store.pathStoreID`:`)
The first sixteen absolute path's initial inline 0 of 16. The actual project has an ULID - on this machine
Only INLINE_0 (graphit-code) and INLINE_1 (private-corpus). All INLINE_2 were directories.
Temporary test.

Cobertura de isolamento antes: **17 de 44 pacotes** tocavam `HOME` em algum teste, com 6
helpers diferentes sob 3 nomes (`withHome`, `isolateHome`, `testHome`), e apenas 3 pacotes
com `TestMain`. `internal/ast` isolava em 4 de 139 arquivos de teste.

The correction: one point, and it's an environment variable

`INLINE_0` is `INLINE_1`, and `INLINE_2` reads `INLINE_3`.
An INLINE_0 in INLINE_1 points to a discardable directory
when INLINE_0 is true.

"Why is the environment variable and not an inline inside `GlobalDir()`: `GlobalDir()`"
It is not the only path to the operator's home. `os.UserHomeDir()` is called directly in 9.
outros lugares — `internal/config` (a config global), `internal/ai` (o cache de 132 MB do
modelo), `internal/hub/adapters/ide`, `cmd/launcher` — e nenhum passa por `GlobalDir()`.
Mover `HOME` cobre todos de uma vez, inclusive os que forem adicionados depois.

And covers two cases that no verification in progress reaches:

1. **Subprocesso.** Um teste que sobe o daemon entrega a ele o ambiente deste processo. O
The child is not binary test-driven, so INLINE\_0 is false there and it would resolve it.
   home real independentemente do que este pacote devolvesse aqui.
Here's the translation:

2. **The Config of Your Own Git.**
    A temporary repository reads `~/.gitconfig`, and in another.

This is a very literal translation, but it maintains the structure and meaning of the original Portuguese text. If you need an idiomatic English version that conveys the same idea more naturally, please let me know!
installation, this configuration names a remote memory – which is like a round of
The tests finish adding a live repository as `origin` and pushing branches of
Test for him ([[The lint that wasn't the GC of Git's remote tests were talking to the real one]]).
   `XDG_CONFIG_HOME` acompanha `HOME` porque o git prefere `$XDG_CONFIG_HOME/git/config`.

A package that isolates itself automatically continues to gain traction: the _INLINE_1_ runs before
Any `TestMain` and of any `t.Setenv`, then it is the floor, never the ceiling.

The cost of importing `testing` into production code: measured

15,625 bytes - 265,379,408 → 265,395,408, **0.006%** of binary. Heavy dependencies
They were already linked by other paths.
The trade-off that seemed like it needed discussion doesn't exist.

What the change revealed: 6 green tests by accident

Os 6 passavam **apenas** porque liam o `~/.graphit` real do desenvolvedor. Todos foram
confirmados verdes no baseline (com `git stash`) e vermelhos com o isolamento — ou seja,
They were not inherent flaws; they were hidden dependencies that isolation revealed.

| Teste | Pacote | Causa real |
|---|---|---|
| `TestEmbeddedLangResolvesAcrossBothBackends` | ast | nenhuma linguagem resolve sem as queries instaladas |
| `TestParsePoolResetsAntlrCachesWithParsesInFlight` | ast | `unknown ANTLR grammar: antlr-plsql` |
| `TestParquetRoundTripPreservesGraph` | ast | grafo vazio porque nada parseou |
| `TestGraphSamplesRunOnAGraphWithoutEntities` | ast | idem |
Brazilian Portuguese to idiomatic English:

"_`TestPrepareASTPublishPrefersParquet`_ | hub | constructs a real graph by publishing artifact AST "
Brazilian Portuguese to idiomatic English:

The daemon used `os.TempDir()` as a "ephemeral" proxy.

The seventh, which only appeared in the complete suite: the identity of Git

`TestGitCLIBackend` (`internal/git`) falhou com

```
git commit failed: exit status 128: fatal: unable to auto-detect email address
(got '<user>@<host>.(none)')
```

Emptying `HOME` is precisely what conceals `~/.gitconfig`, and Git refuses to commit without.
Identity. `internal/memory` and `internal/hub` already resolved this in their own `TestMain`
— eles exportam `GIT_AUTHOR_*`/`GIT_COMMITTER_*` por exatamente esse motivo. Com o
Isolation applies to the 44 packages, and identity became a direct consequence of this.
Here's the translation:

"Inline 0 was responded to there, and not left for each package that has to rediscover."

It only appeared in the complete _INLINE_0_, not in the 12 packages I ran with a focus on
garbage - what is the argument for running the suite entirely before declaring it ready, and not
Just the packages that the change seemed to touch.

The definitions of language are not compiled in binary.

The chain is worth noting because it's not obvious:

```
internal/ast/queries/*.yaml
  → cmd/launcher/runtime/ast/queries/   (cp no Makefile, linha 289)
  → go:embed no launcher                (cmd/launcher/embed.go)
Extracted during installation for ~/.graphit/runtime/<version>/ast/queries/
```

The code reads only the last destination. Binary test never had
Installer, then it starts from zero languages. Before isolation, he finds the runtime of
The version that the developer had installed - which made the green suite dependent on
Machine: A contributor who never installed it and CI would see flaws that don't exist.
reproduzem em lugar nenhum.

The correction is INLINE 0, which seeds the queries **for this checkout** at runtime.
isolated: self-contained. Exercises the path of production load rather than circumventing it, then loader,
Order of Merge and Extension Tables Continue Under Test.

The order is the detail that makes it work. `internal/ast` builds the tables in the background.
Inline 0, own (Inline 1), reading the directory once and caching
The result, including an empty one. INLINE_0 runs after all of INLINE_1, then.
planting there is too late. Sowing lives in the _`init()`_ of _`testsupport`_: dependencies on
A package is initialized before it happens, so import `testsupport` from the test files
It puts this ___ INLINE_3___ in front of the ___ INLINE_4___ — and the ___ INLINE_5___ is.
Here is the translation:

"Inline 0, dependency on Inline 1, in front of both."

## Files Changed

| File | Change | Reason |
|---|---|---|
Here is the translation:

| `internal/brand/testhome.go` | Created | It points to an ephemeral directory and exports the Git identity that empties out `HOME` just as it has been hidden |

This translation aims for idiomatic English while maintaining the original technical meaning.
The inline 0 is created, and a binary test must resolve the home to within the ephemeral root.
Brazilian Portuguese:
| `internal/testsupport/runtimequeries.go` | Created | It seeds the grammar queries for checkout at runtime in the isolated `init()` directory. |
The field was created, but the high failure rate occurred early if planting did not happen.
Brazilian Portuguese:
| `internal/hub/main_test.go` | Modified | The same verification; publishing the AST artifact builds a real graph |

Idiomatic English:
The modified version involves the same verification process, and publishing the Abstract Syntax Tree (AST) artifact results in constructing a genuine graph.
Here is the translation:

"| `internal/daemon/daemon_cwd_test.go` | Modified | The assertion 'not in `os.TempDir()`' has started contradicting the one above it."
| `internal/brand/brand_test.go` | Modified | `TestGlobalDirsAndResolvers` vazava `Brand = "testbrand2"` para todo teste seguinte |
| `Makefile` | Modified | varre os homes da rodada anterior no alvo `test` |

## Trade-offs & Decisions

**`init()` mexendo em `HOME` versus guard em `GlobalDir()`.** Escolhido o `init()`: cobre
os 9 chamadores diretos de `os.UserHomeDir()`, os subprocessos e o git. O guard cobriria
Only those who go through `GlobalDir()`.

Silencing redirection vs bursting. The redirection is silent because of it.
Objective is the result — no test at home real — and a panic would break up 44 packages of
once without anyone having asked for it, but when the temporary directory couldn't
Created, there's panic: the silent fallback on this path is exactly the bug that this
The file exists to remove it, and an `/tmp` that is not _graspable_ will break `t.TempDir()` and almost the entire program.
entire suite

Copy instead of symlink for the queries. The symlink would leave the repository as a copy.
unique and would grab copies in the middle of the session—no value here, because each binary is
teste recebe uma home nova e re-semeia de qualquer forma — enquanto transformaria qualquer
Written in that directory within this checked-out versioned file system.
Today, nobody writes there; copying means that nothing needs to continue being true for
It will go smoothly. There are 45 small files.

The sweep goes before the round, not after. **INLINE_0** does not go **INLINE_1**, and a package.
Without _INLINE_0, there is no hook after _INLINE_1—so nothing can erase
The home on exit. Preparing before limits the residue to equivalent of one round and doesn't tear
o `HOME` de baixo de um processo que um teste vazado deixou rodando, o que varrer depois
faria.

## Technical Debt

The ephemeral homes are not removed at the end of the process—only cleared by `make test`
In the next round, whoever goes straight accumulates a directory by binary.
In `/tmp`, a complete round generated **323 MB** solely in `internal/ast` and `internal/daemon`.
Here is the idiomatic English translation:

"Those 27 packages that were not isolating `HOME` now rely on the floor of `brand`."
Just for that, but it means none of them declare their own need— if they do.
The removal of `init()` results in pollution returning silently. `internal/brand/testhome_test.go`
It is the only thing that secures this door.
- [ ] 8 arquivos de teste em `internal/daemon` usam `os.Setenv("HOME", …)` com `defer` em
It's time for `t.Setenv`; it doesn't match with `t.Parallel()` and will leak if the test fails.
      antes do defer. Com o piso do `brand` no lugar, a maioria deles pode simplesmente ser
      apagada.
The 82 directories have already been removed from the INLINE_1 of the INLINE_0.
Engineer — Returns 0, and only two
      chaves ULID de projeto real permanecem (`01KSH1…` graphit-code, `<private-corpus>` private-corpus).
- [ ] The 2 orphaned branches continue there — this item was closed by
Error in a previous revision of this section, which only verified `path-*` and concluded with.
      resto. `memory/project/test-proj` ainda referencia 4 worktrees em
      `/tmp/TestMemoryService_*` apagados, e `memory/project/validate-test` tem `refs: ["user"]`.
They are remnants of an earlier test before isolation; isolation prevents new ones from being added.
The old one goes to the Engineer, as he is the global lockfile real.
- [ ] `internal/memory/main_test.go` e `internal/hub/main_test.go` continuam exportando a
      identidade do git que o `init()` do `brand` agora exporta para todo mundo. É
Duplication innocuous (identical values) and was intentionally left out because those
Files carry the reasoning of the rest, but it's duplication.

## System Knowledge

The variable is set, and nothing more — without overriding by parameter.
Environment. `HOME` is the sole control point, which is precisely why correction is...
An environment variable.
- **`testing.Testing()` funciona dentro de `init()`.** É um valor que o linker define ao
Build a test binary, and it's already correct before any INLINE_0 gets executed.
The ``make test`` function is no longer available in ``-tags fts5``. The commit ``fb19403`` removed SQLite from the system.
Binary; only historical mention remains in a comment on the Makefile (lines 541-542).
The memory that said the opposite was corrected in this session.
It is a mutable global variable that tests are rewriting. Anything that derives from it.
In call context, it is unstable within the suite; `internal/brand/testhome.go`
The gardener stores the home created in a variable instead of recalculating the root.
The assertion of `daemon_cwd_test.go` was an old proxy. "Not under control."
The global is not in a directory that anyone will delete.
During tests, the proxy started contradicting the assertion when inside `/tmp`.
  imediatamente acima, que exige `cwd == brand.GlobalDir()`.

## Progress Log

### 2026-08-18
Measured the actual residual size: 160 MB, 82 directories, all classified by manifest.
The inline 0 is transient by binary test parent only.
Measured the cost of importing `testing` in production: 15 KiB, 0.6%.
Confirmed with baseline by `git stash` that all six failures were hidden dependencies on
Home of the developer, no regressions.
Inline 0: Query seeding in Inline 1, in the order of initialization.
Empirical Verification: The actual byte-by-byte identical value after 12 packets.
Including those who produced all the original residue.
Final verification session, next round.

The previous session concluded with the suite fully airborne and without any recorded results. Round and.
verificada aqui.

**`make test` completo: 44 pacotes `ok`, zero `FAIL`, zero erro de build.** Nenhum pacote fica
de fora: `GO_PKGS_SKIP` apenas separa as duas passadas (`/antlr/|/treesitter/` sai da passada
with `-race`), and the second round exactly removes the packages that the first one excluded. The changed ones.
mexeu: `internal/ast` 195,6 s (66,1%), `internal/daemon` 52,1 s (79,4%), `internal/brand`
1,0 s (95,5%), `internal/git` 1,1 s (99,2%), `internal/hub` 3,1 s (53,5%).

The real home was not touched. [Inline 0, Inline 1, Inline 2, Inline 3 to]
`maxdepth 3` antes e depois da rodada: `diff` vazio. `memory.lock.json` e `global.lock.json`
Identical. Zero _INLINE_0_ new.

Where was the residue? Comprising 105 homes and 328 MB, each containing
exatamente o que antes ia para a home real:
`home-*/.graphit/{ast/project/path-*,wiki/knowledge/project,models/coderankembed,memory-wt}` —
More than `home-*/.lbdb/extension/` is LadybugDB's extension, also exiting from `$HOME`. A
varredura do `make test` funcionou: 112 homes / 330 MB da rodada anterior foram apagados antes
from the moment it starts.

The identity of Git is false, and **_INLINE_0_** is unattainable. Measured with a probe.
Temporary in `internal/brand`, which made `git init` + `commit` within a `t.TempDir()`. Removed
depois):

"What value does it hold within the binary test?"
|---|---|
| `HOME` | `/tmp/graphit-test-homes/home-3470831534` |
| `XDG_CONFIG_HOME` | `…/home-3470831534/.config` |
| autor e committer do commit | `Test <test@example.com>` |
| `git config --get user.email` | `""` — vazio |
| `git remote -v` | `""` — nada herdado |

The empty line of INLINE_0 is closing both requirements in one go: Git resolves it.
Identity through variables `GIT_AUTHOR_*` and `GIT_COMMITTER_*`, and the global configuration of
Developer - any that may be found in `user.name`/`user.email` on the machine - simply not
There exists there. And that's also why no __`memory.repo` can be inherited, which is the mechanism
The accidental push as described in `docs/tasks/` and in corresponding memory.

`golangci-lint` nos cinco pacotes tocados: **0 issues**.

The measurement of **`/tmp`** is lower than expected: `/tmp` here is `tmpfs` (31 GB, 33% used)
Usage: and the _INLINE_0__ cleans inputs with a 10-day limit. Then, the accumulation of those running
Directly, it has two independent roofs beyond the `make test`.
