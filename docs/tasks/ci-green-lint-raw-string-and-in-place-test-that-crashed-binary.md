Task: Inline 0 returns to being deterministic - raw string lint and inplace test that previously broke the binary

Status completed as of August 7, 2026. Request from Engineer: `make ci` and `make install` at 100%, including what runs on GitHub Actions.

## Estado inicial

The Portuguese table has been translated to idiomatic English as follows:

| Target | Result |
|---|---|
| `make vet` | passed |
| `make lint` | **failed** — 3 found of `staticcheck` |
| `make vulncheck` | passed, 0 vulnerabilities |
| `make ui` / `make ui-lint` | passed (26 warnings, 0 errors) |
| `make install` | passed |
| `make test` | **intermittently failed** — `internal/ast` with SIGSEGV |

Note: The inline codes and markdown formatting have been preserved as they are not translated.

## 1. Lint: raw string in ``regexp.MustCompile`_`

The _INLINE_13_ had three regular expressions written with a string interpreted and double escaped:

```go
rePatternLabel = regexp.MustCompile("([(\\[|]\\s*\\w*\\s*:\\s*)([A-Za-z_]\\w*)")
```

The inline comment 14 rejects this. It is not a preference for the local golangci-lint CI: the CI uses
inline comment 15 with inline comment 16, exactly the version installed here, so the job
inline comment 17 was red due to the same three findings. They passed over raw strings; all three expressions are character by character identical.

### 2. The SIGSEGV: A Test Measuring What Production Abandoned

---

This is an idiomatic English translation of the provided Portuguese text, preserving technical terms and code blocks as requested.

The symptom was __LINE__ 18 in __FILE__ 19, without traceback or any __ERROR__ — the binary terminated after approximately 106 seconds, almost the same time as a single iteration that passes.

Reproduction: 1 crash in 45 executions. Eight isolated package runs, ten with __INLINE_21__/__INLINE_22__/__INLINE_23__/__INLINE_24__ in parallel, twenty-five directed only to cross-process tests, twelve instrumented for coverage — none reproduced. The original crash came from a complete __INLINE_25__.

What closed the diagnosis was not reproduction; it was the question of the Engineer: "Ladybug code does not write and read because that causes problems. Why is it in the test?"

Yes. And the measurement was already in the repository, at the header of `incrementalInPlace` (`internal/ast/incremental_rebuild.go:483`):

```
writer model                                     reads ok  open failed  crashed
in place, commit + CHECKPOINT                      43/60        11         6
copy+swap, production never held by the writer     60/60         0         0
copy+swap as reindexAST does it today              59/60         1         0
```

That's why `inPlaceIncrementalEnabled()` returns `false` unless
`GRAPHIT_INPLACE_INCREMENTAL=1`, and `TestInPlaceIncrementalIsOptIn` sets this default value. The production never allows a reader to open an archive that is being checkpointed.

The inline 32 left. He raised three read-only subprocesses
reading and committing in place on the same file for 10 seconds — the first line of the table, six crashes in sixty. It was the only place in the repository that still ran that, and the header of the file still stated "the daemon writes in-place," a phrase that no longer held true when copy+swap returned to being the default.

A SIGSEGV in Cgo is not an assertion failure; it kills the process, takes along with ~1700 other tests from the package, and reports a line that says nothing. CI red herring about a path that production does not use.

The test was behind the same switch that controls what it measures. With __INLINE_33__, verified, 36553 written, 3 readers, no anomalies, 21.45 seconds.

## Arquivos

Portuguese:
| File | Change | Reason |
|---|---|---|
| `internal/ast/ladybug.go` | Modified | 3 regular expressions for raw string (`staticcheck S1007`) |
| `internal/ast/ladybug_crossprocess_test.go` | Modified | In-place test now requires `GRAPHIT_INPLACE_INCREMENTAL=1`; corrected file header |

English:
The Portuguese text is already in idiomatic English. No translation was necessary.

Verification

The following are all green:
`make vet`, `make lint`, `make vulncheck`, `make ui`, `make ui-lint`, `make test`, `make ci` and
`make install` — all green. The CI (`go test -race -count=1 ./internal/fswatch/... ./internal/ignorer/...`) job
`watcher-cross-platform` was run separately on Linux.

Note: The inline codes and markdown formatting have been preserved as requested.

What was left out

- The motor defect persists. This removes CI from the firing line; it doesn't fix
  `lbug_database_init`. While in-place opt-in and disabled, production does not rely on it.
- `make test` takes a long time and this hasn't been touched. The cost is structural: `internal/ast` of 1,29 million ANTLR-generated lines (`plsql` 408k, `db2` 281k, `tsql` 267k, `postgresql` 193k, `cobol85` 140k) plus two tree-sitter grammars in CGO. `GO_PKGS_SKIP` excludes these packages from being *tested*, not *compiled* — they are linked regardless.
  And `ast.test` compiles everything **twice**, because `-race` and without `-race` are cache entries of distinct types. Add `-covermode=atomic` and `-p 4` on a machine with 20 cores. Addressing this is an individual task, not for here.
- There were 26 ESLint warnings in `internal/ui` (unused imports, one __INLINE_65__, one __INLINE_66__). `npm run lint` exits 0, so neither `make ci` nor the job `UI Build` rejects. The real debt is outside the scope of this request.

Note: Inline references are placeholders for actual code blocks or sections that should be translated into idiomatic English as needed.
