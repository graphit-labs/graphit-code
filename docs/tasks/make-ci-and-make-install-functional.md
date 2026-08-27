# Tarefa: deixar `make ci` e `make install` funcionais

Date: July 28, 2026
Status: Completed

Objective

The two targets needed to go from end to end again. __INLINE_2__ failed; __INLINE_3__ had never been verified on this machine. This log records what was broken, what was corrected, and what was left out of the plan.

The diagnosis

Inline 4 is Inline 5. Running one target per turn:

Portuguese:
| target | result before |
|---|---|
| `ui` | ✓ (npm ci + vite, 7 s) |
| `vet` | ✓ |
| `lint` | ✓ — golangci-lint 2.12.2, 0 issues |
| `vulncheck` | ✓ — 0 vulnerabilities called by the code |
| `test` | ✗ — `internal/ast` failed |
| `ui-lint` | ✓ — 26 warnings, 0 errors (eslint exits with 0) |

English:
The target has been successfully updated using npm ci and vite. This process took only 7 seconds. golangci-lint version 2.12.2 was used without any issues. There were no vulnerabilities detected in the code that could be called by the application. The ESLint tool did not report any errors or warnings during this update.

In other words: a single point of failure, within `test`.
This translation maintains the structure and meaning while using more idiomatic English.

Root cause of `make test`

The three tests of INLINE\_15 failed in the seed, not in the assertion:

```
--- FAIL: TestDeleteRepositoryEmptiesTheGraph (0.04s)
    seed "MATCH (n:Function {uid: 'fnA'}), (p:Parameter {uid: 'paA'}) CREATE (n)-[:HAS_PARAMETER]->(p)":
    ladybug query: Binder exception: Table HAS_PARAMETER does not exist.
```

The previous commit, `763fe938` ("fix: AST indexing schema error"), rewrote
`initSchemaForLabels` (`internal/ast/ladybug.go:275-285`). Previously, the group HAS_PARAMETER was derived from
`CallerLabels` and, when that list was empty, it fell into a literal
`FROM Function TO Parameter`. This fallback was **removed for purpose** — the comment in
`rebuild_index.go:146-150` explains why: it invented a pointer to a rel table pointing to a node table that that corpus never created. The group now exits from
`info.ParamOwnerLabels` and nothing else.

The test, set up by hand by the `sondaSchema`, declared `SchemaInfo` without
`HasParams: true`. Under the new semantics, this means "no edge HAS_PARAMETER", and the seed creates exactly one. The test is out of date; production is correct — `ParamOwnerLabels` populates `rebuild_index.go:151-163` from the real owner of each parameter and filters by `paramOwnerSet` before passing it along.

Note: I've replaced the inline references with underscores (_) to maintain the original structure while translating.

Correction: Declare the owner, as __INLINE_30__ has already done for HAS_FIELD.

## O segundo problema: node_modules dentro do `go list`

It did not knock down CI, but it made CI locally and remotely cover different sets of packages.

The _INLINE_32_ runs __INLINE_33__, and one of the transitive dependencies of the UI loads Go sources after a UI build:

```
$ go list ./... | grep node_modules
github.com/graphit-labs/graphit-code/internal/ui/node_modules/flatted/golang/pkg/flatted
```

As three passed the third-party code before ___INLINE_36__/___INLINE_37__/`test`. On GitHub jobs, this never happens; they only create a placeholder in ___INLINE_39__ and the test job does not run ___INLINE_40__. The ___INLINE_41__ already excluded ___INLINE_42__ — in ___INLINE_43__ and via a linter rule — so the lint was consistent, and the Go tools were not.

The package filters turned into two variables at the top of the Makefile, used in three places that previously repeated the grep chain.

Verification of `make install`

The inline 45 depends on the inline 46 → inline 47 (inline 48). Passes with warm caches of inline 49. The binary exits at ~517 MB because it embeds model, ONNX Runtime, and ICU.

The default is `PREFIX`, which cannot be user-gravable here — the target falls on branch `/usr/local/bin` and asks for a password, which does not run non-interactively. The verification was done by the gravable branch, and the installed binary was executed with `sudo cp` false to prevent reinstallation of `HOME` nor `~/.graphit/runtime/v0.1.27` with the daemon running inside it:

```
make install PREFIX=/tmp/graphit-install-test
HOME=/tmp/gt-fakehome /tmp/graphit-install-test/graphit --version   # graphit version v0.1.27
```

The runtime extracted everything (`graphit-core`, `graphit-mcp`, `liblbug.so`, `libonnxruntime.so`,
`models/`, `ast/`) and the core responded. The build → copy → extraction → execution path is functional; the branch with `sudo` is identical to `cp` with privilege, and `⚠ not in PATH` only triggers when `PREFIX` actually isn't in PATH — this was the case of the temporary directory.

## Resultado

```
make ci
  ✅ All CI checks passed.
```

Without any line `FAIL` in the log. `coverage.out` is generated (it's the file that the GitHub job converts to Coverage).

## Arquivos alterados

| File | Modified | Reason |
|---|---|---|
| `internal/ast/writer_delete_repository_test.go` | Updated | `sondaSchema` gained `ParamOwnerLabels: []string{"Function"}` and the reason why the fallback no longer exists |
| `Makefile` | Updated | `GO_PKGS_SKIP` / `GO_PKGS_PARSERS` at the top; `vet` and `test` start using them, excluding `node_modules` |

Decisions and trade-offs

- "Correct the test, not the production." The fallback removed in `763fe938` was the bug, not its removal. Reintroducing it would cause the schema to declare a point for an nonexistent node table, which is exactly what aborts the rebuild on Oracle's corpus.
- "Filter `node_modules` in the Makefile instead of leaving it as is." It's small and aligns with the `.golangci.yml` decision already made. Without this, `make ci` continues to pass today but depends on the health of a Node Package Manager (NPM) package to continue passing tomorrow — and the failure would only appear on the developer’s machine.
- "Variables instead of repeating the grep." The chain appeared in three places; it was like that it diverged in the next filter.

Technical Debit

- [ ] `BUILD_ID ?= $(shell …)` is a recursive variable, so the `$(shell)` runs on each expansion:
   The three `go build` of `build-linux` receive different UUIDs (visible in the build log).
   Today is harmless — `version.BuildID` only reads it by itself, which compares the stamp with the compiled value inside it (`cmd/launcher/main.go:205-231`), and nothing crosses the BuildID of the core with that of the launcher. Fixing would be `BUILD_ID := $(BUILD_ID)` after `?=`, preserving override via env.
- [ ] `.github/workflows/ci.yml` installs Zig 0.16.0 on job `build-check`, but nothing in `build-linux` uses Zig — no `CC`/`CXX` points to it. Likely leftover; removing shortens the job.
- [ ] `build-linux` packs all `libicu*.so.[0-9]*` found in `/usr/lib` and `/lib`: on this machine, sonames 74 **and** 78 enter, along with `libicutest` and `libicutu`, which the binary does not use. The weight is under 517 MB.
- [ ] `install` runs `mkdir -p $(PREFIX)` without `sudo` before deciding whether it needs `sudo` on `cp`. If the `PREFIX` does not exist and the parent directory is not writable, make aborts there, before reaching the privileged branch. It affects a new `PREFIX` in root path.

This translation preserves the original structure, code blocks, markdown formatting, file paths, and technical terms as requested.

## Conhecimento do sistema

- **A custom inline mount must declare the owners.** Alone, neither ___INLINE_110__ nor ___INLINE_111__ creates HAS_PARAMETER in the same way that alone does not create HAS_FIELD. Both groups exit only from the lists of owners (___INLINE_112__, ___INLINE_113__), filtered by `nodeTables`. Any future test building a schema directly falls into the same trap.
- **`make ui` changes the result of `go list ./...`.** This is the only reason why the order of targets inside `ci` matters for the tested set of packages.
- **The launcher resolves appDir by `os.UserHomeDir()`, that is, `$HOME`. This is what allows testing a newly installed binary without touching `~/.graphit` — and also what makes it risky to run with the `HOME` real being: `cleanupOldRuntimes()` wipes out runtime directories of other versions.
- **`ui-lint` passes with warnings.** There are 26 (unused imports, one `any`, one `react-hooks/exhaustive-deps`) and ESLint exits 0, so they do not block. They are not new.

Note: Inline code blocks have been preserved as is.
