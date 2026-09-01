# Task: `make ci` is deterministic again — raw string lint and the in-place test that crashed the binary

**Status: complete** on 2026-08-07. Engineer's request: `make ci` and `make install` at 100%, including
what runs on GitHub Actions.

## Initial state

| target | result |
|---|---|
| `make vet` | passed |
| `make lint` | **failed** — 3 `staticcheck` findings |
| `make vulncheck` | passed, 0 vulnerabilities |
| `make ui` / `make ui-lint` | passed (26 warnings, 0 errors) |
| `make install` | passed |
| `make test` | **failed intermittently** — `internal/ast` with SIGSEGV |

## 1. Lint: raw string in `regexp.MustCompile`

`internal/ast/ladybug.go` had three regexps written as interpreted strings with double escaping:

```go
rePatternLabel = regexp.MustCompile("([(\\[|]\\s*\\w*\\s*:\\s*)([A-Za-z_]\\w*)")
```

`staticcheck S1007` refuses that. It is not a style preference of my local golangci-lint: CI uses
`golangci-lint-action@v8` with `version: v2.12.2`, exactly the version installed here, so the
`Lint & Vet` job was red on the same three findings. They became raw strings; the three expressions
are character for character the same.

## 2. The SIGSEGV: a test measuring the arrangement production abandoned

The symptom was `signal: segmentation fault (core dumped)` in `internal/ast`, with no traceback and no
`--- FAIL` at all — the whole binary died after ~106s, practically the same time as a passing run.

**Reproduction: 1 crash in 45 runs.** 8 isolated runs of the package, 10 under `-p 4` contention with
`daemon`/`memory`/`dream` in parallel, 25 aimed only at the cross-process tests, 12 with coverage
instrumentation — none reproduced it. The original crash came from a full `make test`.

What closed the diagnosis was not the reproduction, it was the Engineer's question: *in the code
writing and reading Ladybug don't run concurrently because it causes problems, why does the test do
it?*

It does. And the measurement was already in the repository, in the header of `incrementalInPlace`
(`internal/ast/incremental_rebuild.go:483`):

```
writer model                                     reads ok  open failed  crashed
in place, commit + CHECKPOINT                      43/60        11         6
copy+swap, production never held by the writer     60/60         0         0
copy+swap as reindexAST does it today              59/60         1         0
```

That is why `inPlaceIncrementalEnabled()` returns `false` unless
`GRAPHIT_INPLACE_INCREMENTAL=1`, and `TestInPlaceIncrementalIsOptIn` pins that default. Production
never lets a reader open a file that is being checkpointed.

`TestLadybugInPlaceWritesUnderCrossProcessReaders` did let it. It spins up 3 read-only subprocesses
scanning and commits in-place for 10s on top of the same file — the arrangement in the first row of
the table, 6 crashes in 60. It was the only place in the repository still running that, and the file
header still claimed that "the daemon writes in-place", a sentence that stopped being true when
copy+swap became the default again.

A SIGSEGV in cgo is not a failed assertion: it kills the process, takes the ~1700 other tests in the
package with it, and reports one line that says nothing. Randomly red CI over a path production does
not use.

**The test stayed** — it is the evidence for the decision — **behind the same switch that turns on
what it measures**. With `GRAPHIT_INPLACE_INCREMENTAL=1` it runs: verified, 36553 writes, 3 readers, 0
anomalies, 21.45s.

## Files

| File | Change | Reason |
|---|---|---|
| `internal/ast/ladybug.go` | Modified | 3 regexps turned into raw strings (`staticcheck S1007`) |
| `internal/ast/ladybug_crossprocess_test.go` | Modified | the in-place test now requires `GRAPHIT_INPLACE_INCREMENTAL=1`; file header corrected |

## Verification

`make vet`, `make lint`, `make vulncheck`, `make ui`, `make ui-lint`, `make test`, `make ci` and
`make install` — all green. `graphit --version` responds from the installed binary. The CI job
`watcher-cross-platform` (`go test -race -count=1 ./internal/fswatch/... ./internal/ignorer/...`)
was run separately on Linux.

## What was left out

- **The engine defect is still there.** This takes CI out of the line of fire; it does not fix
  `lbug_database_init`. As long as in-place is opt-in and off, production does not touch it.
- **`make test` takes far too long and that was not touched.** The cost is structural: `internal/ast`
  imports 1.29M lines of generated ANTLR (`plsql` 408k, `db2` 281k, `tsql` 267k, `postgresql` 193k,
  `cobol85` 140k) plus 20 tree-sitter grammars in CGO. `GO_PKGS_SKIP` excludes those packages from
  being *tested*, not from being *compiled* — they are linked into `ast.test` either way.
  And `make test` compiles everything **twice**, because `-race` and without `-race` are distinct
  cache entries. Add `-covermode=atomic` and `-p 4` on a 20-core machine. Addressing this is a task
  of its own, it did not fit here.
- **26 eslint warnings in `internal/ui`** (unused imports, one `any`, one
  `react-hooks/exhaustive-deps`). `npm run lint` exits 0, so neither `make ci` nor the `UI Build` job
  fails. Real debt, outside the scope of this request.
