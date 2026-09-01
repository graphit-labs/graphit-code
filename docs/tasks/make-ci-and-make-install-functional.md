# Task: make `make ci` and `make install` functional

**Date:** 2026-07-28
**Status:** completed

## Objective

Both targets needed to pass end to end again. `make ci` was failing; `make install` had never been
verified on this machine. This log records what was broken, what was fixed and what was deliberately
left out.

## The diagnosis

`make ci` is `ui vet lint vulncheck test ui-lint`. Running one target at a time:

| target | result before |
|---|---|
| `ui` | ✓ (npm ci + vite, 7 s) |
| `vet` | ✓ |
| `lint` | ✓ — golangci-lint 2.12.2, 0 issues |
| `vulncheck` | ✓ — 0 vulnerabilities called by the code |
| `test` | ✗ — `internal/ast` failed |
| `ui-lint` | ✓ — 26 warnings, 0 errors (eslint exits 0) |

That is: a single point of failure, inside `test`.

## Root cause of `make test`

The three tests in `internal/ast/writer_delete_repository_test.go` were dying in the seed, not in the
assertion:

```
--- FAIL: TestDeleteRepositoryEmptiesTheGraph (0.04s)
    seed "MATCH (n:Function {uid: 'fnA'}), (p:Parameter {uid: 'paA'}) CREATE (n)-[:HAS_PARAMETER]->(p)":
    ladybug query: Binder exception: Table HAS_PARAMETER does not exist.
```

The previous commit, `763fe938` ("fix: AST indexing schema error"), rewrote
`initSchemaForLabels` (`internal/ast/ladybug.go:275-285`). Before, the HAS_PARAMETER group was
derived from `CallerLabels` and, when that list came in empty, fell back to a literal
`FROM Function TO Parameter`. That fallback was **removed on purpose** — the comment in
`rebuild_index.go:146-150` explains why: it invented a rel table endpoint pointing at a node table
that that corpus never created. The group now comes from `info.ParamOwnerLabels` and from nothing
else.

`sondaSchema`, the `SchemaInfo` built by hand in the test, declared `HasParams: true` without
`ParamOwnerLabels`. Under the new semantics that means "no HAS_PARAMETER edge", and the seed creates
exactly one. The test was out of date; production is correct — `rebuild_index.go:151-163` populates
`paramOwnerSet` from the real owner of each parameter and filters by `labelSet` before passing it
along.

Fix: declare the owner, the way `FieldOwnerLabels` already did for HAS_FIELD.

## The second problem: node_modules inside `go list`

It was not bringing CI down, but it made local CI and remote CI cover different sets of packages.

`make ui` runs `npm ci`, and one of the UI's transitive dependencies ships Go sources. After a UI
build:

```
$ go list ./... | grep node_modules
github.com/graphit-labs/graphit-code/internal/ui/node_modules/flatted/golang/pkg/flatted
```

Since `ci` runs `ui` **before** `vet`/`lint`/`test`, all three started covering third-party code. In
the GitHub jobs this never happens: they only create a placeholder in `internal/ui/dist` and the test
job does not run `npm ci`. `.golangci.yml` already excluded `node_modules` — in `exclusions.paths`
and in a per-linter rule — so lint was consistent and the go tools were not.

The package filters became two variables at the top of the Makefile, used in the three places that
previously repeated the chain of greps.

## Verification of `make install`

`install` depends on `build` → `build-linux` (`ui setup-lbug fetch-ort-linux fetch-model`). It
passes, with the `/tmp` caches warm. The binary comes out at `.build/graphit-linux-amd64` at
~517 MB, because it embeds the model, ONNX Runtime and ICU.

The default `PREFIX` is `/usr/local/bin`, which is not writable by the user here — the target falls
into the `sudo cp` branch and asks for a password, which does not run non-interactively. The
verification was done through the writable branch, and the installed binary was run with a fake
`HOME` so as not to rewrite `~/.graphit/runtime/v0.1.27` or the `launcher.stamp` with the daemon
running out of it:

```
make install PREFIX=/tmp/graphit-install-test
HOME=/tmp/gt-fakehome /tmp/graphit-install-test/graphit --version   # graphit version v0.1.27
```

The runtime extracted completely (`graphit-core`, `graphit-mcp`, `liblbug.so`, `libonnxruntime.so`,
`models/`, `ast/`) and the core answered. The build → copy → extraction → exec path is functional;
the branch with `sudo` is the same `cp` with privilege, and the `⚠ não está no PATH` only fires when
`PREFIX` really is not in PATH — which was the case for the temporary directory.

## Result

```
make ci
  ✅ All CI checks passed.
```

Without a single `FAIL` line in the log. `coverage.out` is generated (it is the file the GitHub job
converts to Cobertura).

## Files changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/writer_delete_repository_test.go` | Modified | `sondaSchema` gained `ParamOwnerLabels: []string{"Function"}` and the comment on why the fallback no longer exists |
| `Makefile` | Modified | `GO_PKGS_SKIP` / `GO_PKGS_PARSERS` at the top; `vet` and `test` now use them, excluding `node_modules` |

## Decisions and trade-offs

- **Fix the test, not production.** The fallback removed in `763fe938` was the bug, not its removal.
  Reintroducing it would make the schema declare an endpoint to a nonexistent node table again,
  which is exactly what was aborting the rebuild on the Oracle corpus.
- **Filter `node_modules` in the Makefile instead of leaving it as is.** It is small and it aligns
  the go tooling with `.golangci.yml`, which had already made that decision. Without it, `make ci`
  keeps passing today but depends on the health of an npm package to keep passing tomorrow — and the
  failure would show up only on the dev's machine.
- **Variables instead of repeating the grep.** The chain appeared in three places; that is how it
  would have diverged at the next filter.

## Technical debt

- [ ] `BUILD_ID ?= $(shell …)` is a recursive variable, so the `$(shell)` runs on every expansion:
      the three `go build` invocations in `build-linux` get different UUIDs (visible in the build
      log). Today it is harmless — `version.BuildID` is only read by the launcher itself, which
      compares the stamp against the value compiled inside it (`cmd/launcher/main.go:205-231`), and
      nothing crosses the core's BuildID with the launcher's. The fix would be
      `BUILD_ID := $(BUILD_ID)` after the `?=`, preserving override by env.
- [ ] `.github/workflows/ci.yml` installs Zig 0.16.0 in the `build-check` job, but nothing in
      `build-linux` uses Zig — no `CC`/`CXX` points at it. Probably a leftover; removing it
      shortens the job.
- [ ] `build-linux` packs every `libicu*.so.[0-9]*` it finds in `/usr/lib` and `/lib`: on this
      machine sonames 74 **and** 78 get in, plus `libicutest` and `libicutu`, which the binary does
      not use. Dead weight inside the 517 MB.
- [ ] `install` runs `mkdir -p $(PREFIX)` without `sudo` before deciding whether it needs `sudo` for
      the `cp`. If `PREFIX` does not exist and the parent directory is not writable, make aborts
      right there, before reaching the privileged branch. It does not affect the default
      (`/usr/local/bin` already exists), it affects a new `PREFIX` under a root-owned path.

## System knowledge

- **A `SchemaInfo` built by hand has to declare the owners.** `HasParams: true` on its own does not
  create HAS_PARAMETER, in the same way that `HasFields: true` on its own does not create HAS_FIELD.
  Both groups come only from the owner lists (`ParamOwnerLabels`, `FieldOwnerLabels`), filtered by
  `nodeTables`. Any future test that builds a schema directly falls into the same trap.
- **`make ui` changes the result of `go list ./...`.** It is the only reason the order of the targets
  inside `ci` matters for the set of packages that get tested.
- **The launcher resolves its appDir through `os.UserHomeDir()`**, that is, `$HOME`. That is what
  makes it possible to test a freshly installed binary without touching `~/.graphit` — and it is
  also what makes running it with the real `HOME` dangerous: `cleanupOldRuntimes()` deletes the
  runtime directories of other versions.
- **`ui-lint` passes with warnings.** There are 26 (unused imports, one `any`, one
  `react-hooks/exhaustive-deps`) and eslint exits 0, so they do not block. They are not new.
