# `make ci` passes again, and `make install` was actually executed

**Date:** 2026-07-28
**Scope:** `internal/ast/writer_delete_repository_test.go`, `Makefile`
**Origin:** Engineer's request — both targets need to be functional

---

## What was broken

Running one target of `ci: ui vet lint vulncheck test ui-lint` at a time, only one failed:

| target | before |
|---|---|
| `ui` | ✓ npm ci + vite, 7 s |
| `vet` | ✓ |
| `lint` | ✓ golangci-lint 2.12.2, 0 issues |
| `vulncheck` | ✓ 0 vulnerabilities reachable via code |
| `test` | ✗ `internal/ast` |
| `ui-lint` | ✓ 26 warnings, 0 errors — eslint exits 0 |

And the failure wasn't an assertion, it was **seed**:

```
--- FAIL: TestDeleteRepositoryEmptiesTheGraph (0.04s)
    seed "MATCH (n:Function {uid: 'fnA'}), (p:Parameter {uid: 'paA'}) CREATE (n)-[:HAS_PARAMETER]->(p)":
    ladybug query: Binder exception: Table HAS_PARAMETER does not exist.
```

All three tests in `writer_delete_repository_test.go` died on the same line — the
`HAS_PARAMETER` rel table didn't exist in the database the test itself had just created.

## The cause: the fallback that `763fe938` removed on purpose

`763fe938` ("fix: AST indexing schema error") rewrote `initSchemaForLabels`
(`internal/ast/ladybug.go:275-285`). The old version derived parameter owners from
`CallerLabels` and, when that list came empty, guessed a literal:

```go
if len(paramRels) == 0 {
    paramRels = append(paramRels, "FROM `Function` TO `Parameter`")
}
```

That guess **was the bug**. The comment in `rebuild_index.go:146-150` says why: it
declared an endpoint pointing to a node table that corpus never created, and a rejected DDL
aborts the entire rebuild. Now the group comes from `info.ParamOwnerLabels` and nothing
else — if the list is empty, the group is not created.

`sondaSchema`, the hand-built `SchemaInfo` in the test, declared `HasParams: true` **without**
`ParamOwnerLabels`. Under the new semantics that means "no HAS_PARAMETER edge", and the
seed creates exactly one.

What was outdated was the test. Production is correct: `rebuild_index.go:151-163`
populates `paramOwnerSet` from the real owner of each parameter — the parameter's `FuncUID` and
the `ParentLabel` of CONTAINS edges — and filters by `labelSet` before forwarding. Reintroducing
the fallback would make the schema declare the phantom endpoint again, which is the defect that broke
rebuild on the 35,358-file Oracle corpus.

The fix is to declare the owner, as `FieldOwnerLabels` already did for `HAS_FIELD`:

```go
ParamOwnerLabels: []string{"Function"},
```

The rule left for any future test that builds `SchemaInfo` directly: `HasParams: true`
alone creates nothing, and `HasFields: true` alone also doesn't. Both groups come only from
owner lists, filtered by `nodeTables`.

## The second defect: `node_modules` entering `go list`

It didn't break CI. It made local and remote CI cover different package sets, which
is worse, because the divergence only appears on the dev machine.

`make ui` runs `npm ci`, and one of the UI's transitive dependencies carries Go sources. After a
UI build:

```
$ go list ./... | grep node_modules
github.com/graphit-labs/graphit-code/internal/ui/node_modules/flatted/golang/pkg/flatted
```

And `ci` runs `ui` **before** `vet`/`lint`/`test` — order that came from the previous fix, because
`vet` needs `internal/ui/dist`. Consequence: all three targets started covering third-party code. On GitHub jobs this never happens; they create a placeholder in
`internal/ui/dist` and the test job doesn't run `npm ci`.

`.golangci.yml` had already made that decision — `node_modules` appears in
`exclusions.paths` **and** in a per-linter rule. That is, lint was consistent and go
tools were not. Filters became two variables at the top of `Makefile`, used in the three
places that previously repeated the grep chain:

```make
GO_PKGS_SKIP    := /antlr/|/treesitter/|/node_modules/
GO_PKGS_PARSERS := /antlr/|/treesitter/
```

The parser-generated pass also filters `node_modules`, because nothing guarantees an npm package
won't have `/treesitter/` in the path. Count after change: **38** packages in the race
pass, **21** in the parsers pass, none from `node_modules`, with or without `npm ci` having run.

## `make install`, which the previous changelog left unexecuted

The `20260728_indexing_writes_to_correct_project.md` changelog ends with: *"`make install` was not
executed: depends on `make build` (exit 0, verified) and a `cp` to `/usr/local/bin`,
which is not writable by the user and would require `sudo`."* Now it was.

The default `PREFIX` still requires a password, so verification went through the writable branch — the
same `cp`, without privilege:

```
make install PREFIX=/tmp/graphit-install-test
  ✓ Installed to /tmp/graphit-install-test/graphit          (517 MB)
```

And the installed binary was executed with a fake `HOME`, not the real one. This isn't pedantry: the launcher resolves its `appDir` via `os.UserHomeDir()` (`cmd/launcher/main.go:23-31`), and the
built version is the same `v0.1.27` that's installed — running it with the real `HOME` would rewrite
`~/.graphit/runtime/v0.1.27` and the `launcher.stamp`, with `cleanupOldRuntimes()` deleting the
other versions' directories, all with the daemon running from inside that directory.

```
HOME=/tmp/gt-fakehome /tmp/graphit-install-test/graphit --version
graphit version v0.1.27
```

Runtime extracted fully — `graphit-core`, `graphit-mcp`, `liblbug.so`, `libonnxruntime.so`,
`models/`, `ast/` — and core responded. The build → copy → extraction → core exec path is
functional end to end.

## Verification

```
make ci
  ✅ All CI checks passed.
```

No `FAIL` line in the log — which here is not a formality: the `test` target only started
propagating exit code in the previous fix, and before it printed success with 30 failures on screen.
`coverage.out` is generated (32 MB, it's the file the GitHub job converts to Cobertura).

```
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go test -race -tags fts5 -run TestDeleteRepository -count=1 ./internal/ast/
  ok  (1.3 s, 3/3 PASS)
```

## Remaining debt

None of this breaks the two targets today; all were found looking at the build and remain
recorded in Graphit Task `tsk-4761f865d2f7`.

- **`BUILD_ID` is recalculated on every expansion.** `BUILD_ID ?= $(shell …)` creates a recursive
  variable, so `$(shell)` runs again on each use: the three `go build` of `build-linux`
  get different UUIDs, visible in the log. Harmless today — `version.BuildID` is only read
  by the launcher itself, comparing stamp with the value compiled inside it
  (`cmd/launcher/main.go:205-231`), and nothing crosses core's BuildID with launcher's. Fixing
  is `BUILD_ID := $(BUILD_ID)` after `?=`, preserving env override.
- **Zig left over in CI.** `.github/workflows/ci.yml` downloads and installs Zig 0.16.0 in the
  `build-check` job, but no `CC`/`CXX` of `build-linux` points to it. Leftover; removing
  shortens the job.
- **Bundled ICU in excess.** `build-linux` copies every `libicu*.so.[0-9]*` it finds in
  `/usr/lib` and `/lib`: on this machine sonames **74 and 78** enter, plus `libicutest` and
  `libicutu`, which the binary doesn't use. Dead weight inside the 517 MB.
- **`install` does `mkdir -p $(PREFIX)` without `sudo`** before deciding whether `sudo` is needed for
  `cp`. With a non-existent `PREFIX` under a root path, make aborts there, before reaching the
  privileged branch. Doesn't affect the default, which already exists.
