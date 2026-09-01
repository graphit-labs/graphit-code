---
title: "`make ci` and every GitHub CI check green, correct and adequately fast"
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [ci, github-actions, lint, performance, makefile]
---

# `make ci` and every GitHub CI check green, correct and adequately fast

## Objective
The engineer asked for two things at once, and the second is not implied by the first:

1. **`make ci` must pass**, end to end, locally.
2. **Every verification that runs in GitHub CI must work correctly** — which means the
   workflow must actually be *valid and executed*, must check *the same build* the local
   loop checks, and must do it with adequate performance.

"Correctly" is the operative word: a job that is skipped, that lints a different build
configuration than the one that ships, or that a whole invalid workflow file prevents from
ever starting, is not a passing check — it is an absent one. So the scope is
`Makefile`, `.golangci.yml`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
(its CI gate runs the same checks and drifts from `ci.yml`), plus whatever source fixes the
checks demand.

## Reasoning
What memory and the wiki already establish, and what this task therefore does **not** need to
rediscover:

- `docs/tasks/optimize-make-ci.md` (status `done`, 2026-08-27) already measured the local
  targets, parallelised `ci` with `$(MAKE) -j4`, and introduced `test-short` +
  `testing.Short()` skips for the 132 MB model download and the heavy LanceDB gate. That work
  stands; this task builds on it rather than redoing it.
- Memory `make test é lento por estrutura` records the structural cost: `internal/ast`
  transitively links 1.29 M lines of generated ANTLR plus 20 tree-sitter CGO grammars, and
  `make test` compiles it **twice** (race pass and no-race parser pass are distinct cache
  entries). `GO_PKGS_SKIP` excludes those packages from being *tested*, not from being
  *linked*. So local test wall time is dominated by compile+link, not by test execution.
- Memory `make ci quebrava no vet por código gerado do ANTLR` records why `vet` carries
  `-unreachable=false` and `GO_PKGS_SKIP`.

What measurement in this session added, before any edit (see `## Progress Log`):

- `make lint` **fails**: 27 issues, so `make ci` is red at its second parallel target.
- `.github/workflows/ci.yml` is **not a valid workflow**: `permissions.code-quality` is not a
  GitHub permission scope. GitHub rejects the file, so *none* of its six jobs has been
  running. This is the single largest finding — every other CI defect below was invisible
  because of it.
- The lint configuration and the lint job disagree with the Makefile about which build is
  being checked: `.golangci.yml` pins `build-tags: [fts5]`, a tag the Makefile's own comment
  declares dead ("SQLITE IS GONE, and with it the `fts5` tag"), while `make lint` passes
  `--build-tags lancedb` and the GitHub job passes no tags at all.

## Justification
- **Alternative A — only fix `make lint` and stop.** Rejected: it satisfies request (1) and
  leaves request (2) entirely unmet, since the workflow file would still be rejected by GitHub
  and no check would run at all.
- **Alternative B — rewrite the test suite for dependency injection so `make test` stops
  needing ORT/LanceDB/network.** Correct in the long run and already recorded as debt by the
  previous task, but far outside a "make CI green and fast" change; it stays in the backlog.
- **Chosen** — treat the pipeline as the deliverable: make the workflow *valid*, make every
  job check the *same build configuration* as the local loop, fix the source issues those
  checks legitimately report, and take the performance win where it is free (caching the
  invariant downloads, dropping leftover setup steps, parallelism) rather than by removing
  coverage.

## Plan & Task Breakdown

- [x] **T0 — Measure before editing** — Time and record the exit code of every `ci` target in
  isolation, and validate all three workflow files with `actionlint`.
  Spec: numbers and exit codes land in `## Progress Log`; no source file touched yet.
- [x] **T1 — Make `ci.yml` a valid workflow** — Remove/replace the invalid
  `permissions.code-quality` scope. Spec: `actionlint .github/workflows/*.yml` exits 0 with no
  diagnostics. Constraint: the remaining scopes must still cover what the jobs do (Codecov
  upload needs no write scope; it authenticates with `CODECOV_TOKEN`).
- [x] **T2 — One build configuration for every checker** — Reconcile `.golangci.yml`,
  `make lint`, `make vet`, `make vulncheck` and the GitHub `lint`/`security` jobs onto
  `-tags lancedb`. Spec: the dead `fts5` tag is gone from `.golangci.yml`; the GitHub lint job
  lints with `lancedb`; `govulncheck` in the `security` job carries the tag the Makefile
  comment says it needs. Constraint: `internal/lancestore/cgo_lancedb.go` and the rest of the
  `lancedb`-gated code must be *inside* what lint analyses — that is the point.
- [x] **T3 — Fix the 27 issues `make lint` reports** — 21 × `unused`, 2 × `errorlint`,
  2 × `ineffassign`, 1 × `staticcheck` SA4004, 1 × `gocritic` dupBranchBody. Spec: each is
  resolved on its merits — dead code deleted, not silenced with `//nolint` — and
  `make lint` exits 0. Constraint: `unused` in `internal/ast/rebuild_index.go` covers a large
  block of `*JSON` methods; before deleting, confirm via the AST graph that nothing (including
  another repository in the ecosystem) calls them.
- [x] **T4 — `make test` green** — Spec: `make test` exits 0 and writes `coverage.out`.
- [x] **T5 — CI performance** — Spec: every job caches what it re-downloads and skips what it
  does not use. Concretely: the `test` job caches ORT + liblbug + the model cache (today it
  re-fetches all three every run, while `build-check` already caches two of them); the
  leftover Zig setup in `build-check` is removed (already recorded as debt — no `CC`/`CXX`
  points at it); the `lint` job stops paying for a Rust build it may not need.
- [x] **T6 — Align the release CI gate** — Spec: `release.yml`'s `ci` job runs the same checks,
  with the same tags and the same caches, as `ci.yml`. Constraint: it gates every artifact, so
  it must not become *weaker* than `ci.yml`.
- [x] **T7 — Verify** — Spec: `actionlint` 0, `make vet` 0, `make lint` 0, `make ui-lint` 0,
  `make vulncheck` 0, `make test` 0, `make ci` prints its success banner. Record wall times
  for each next to the T0 baseline.

## Implementation Details
_(filled in as each task lands — see `## Progress Log`)_

## Use Cases
_(filled in as the work lands)_

## Test Cases & Acceptance Criteria
_(filled in as the work lands)_

## Files Changed
| File | Change | Reason |
|---|---|---|
| `docs/tasks/make-ci-and-github-ci-green-and-fast.md` | Created | Opens this task before the first edit |

## Trade-offs & Decisions
_(filled in as the work lands)_

## Technical Debt
Carried in from `docs/tasks/optimize-make-ci.md`, still open and still out of scope here:
- [ ] `internal/ai` and `internal/lancestore` heavy tests use real disk/network instead of an
  injected `ModelManager`/`Store`.
- [ ] `vulncheck` sits in the full `ci`; its 120 s network cost is the slowest single target.
- [ ] `BUILD_ID ?= $(shell …)` is a recursive variable, so each expansion re-runs `$(shell)`
  and the three `go build` invocations of `build-linux` get different UUIDs.

## System Knowledge
- **An invalid workflow file is indistinguishable from a passing one at a glance.** GitHub
  rejects `ci.yml` outright over one unknown `permissions` key, and the repository simply shows
  no CI runs rather than a failure. `actionlint` catches it in milliseconds and belongs in the
  local loop for exactly this reason.
- **`fts5` is dead but still load-bearing in configuration.** `Makefile:43` documents that
  SQLite and its `fts5` tag are gone; `.golangci.yml:4` still pins it. Two comments in the
  Makefile (`:828`, `:852`) explain the failure mode a wrong tag produces: `internal/ast` and
  `internal/wiki` resolve to a guard file instead of the package, so the checker both misses
  every real diagnostic *and* stops on an undefined guard symbol — which reads like a broken
  tool rather than a missing flag.

## Progress Log

### 2026-09-01
- Task log created before any edit, per the knowledge protocol.
- Memory and wiki consulted first: `optimize-make-ci` task page, `make test é lento por
  estrutura` and `make ci quebrava no vet` memories. Their conclusions are reused, not
  re-derived.
- **T0 baseline, this machine (20 cores / 61 GB), each target in isolation:**

  | target | exit | wall |
  |---|---|---|
  | `make lancedb-native` | 0 | 0 s (cached native present) |
  | `make vet` | 0 | 17 s |
  | `make lint` | **2** | 15 s |
  | `make ui` | 0 | 21 s |
  | `make ui-lint` | 0 | 4 s (3 warnings, 0 errors) |

- **`make lint` breakdown of the 27 issues:** `unused` 21 (20 in `internal/ast`, mostly
  `*JSON` methods on `rebuildIndex`, plus `nodeRowsFor`, `appendArrowValueDirect`,
  `firstDDLLine`, and a test helper `bundleSchema`), `errorlint` 2
  (`internal/ast/pipeline.go:855,919` — `%v` on a rollback error inside a wrapping
  `fmt.Errorf`), `ineffassign` 2 (`internal/ast/pipeline.go:689,825`), `staticcheck` SA4004 1
  (`internal/ast/node_columns_test.go:60` — unconditional `break` in a loop), `gocritic`
  dupBranchBody 1 (`cmd/graphit/commands/runners.go:407`).
- **`actionlint` on all three workflows:** exactly one diagnostic, and it is fatal —
  `ci.yml:11:3 unknown permission scope "code-quality"`. GitHub rejects the file, so the
  `lint`, `test`, `platform-semantics`, `security`, `build-check` and `ui-build` jobs have not
  been running at all. Everything else found in CI below was masked by this.
- **Divergences found by reading the workflows against the Makefile** (not yet fixed):
  - `security` job runs `govulncheck ./...` with **no build tags**, while `make vulncheck`
    passes `-tags lancedb` and the Makefile comment states that without it the load aborts on
    an undefined guard symbol before reporting anything.
  - `lint` job uses `golangci/golangci-lint-action@v8` with `args: --timeout=5m` and no
    `--build-tags`, so it falls back to `.golangci.yml`'s `build-tags: [fts5]` — a dead tag —
    while `make lint` uses `lancedb`. Local and remote lint therefore analyse different code.
  - `test` job caches nothing for ONNX Runtime, liblbug or the model, though `make test`
    depends on `setup-lbug` and `$(ORT_HOST_FETCH)` and downloads a 132 MB model. The
    `build-check` job already caches the first two — the pattern exists, it just was not
    applied here.
  - `build-check` installs Zig 0.16.0 and nothing consumes it (already recorded as debt in
    `docs/changelogs/20260728_make_ci_and_make_install_functional.md`).
- Next: T1 (workflow validity), then T2 (one build configuration), then T3 (the 27 issues).

- **T1 done — `ci.yml` is a valid workflow.** `permissions` is now `contents: read` alone.
  Codecov authenticates with `CODECOV_TOKEN`, so no write scope is needed by anything in the
  file. `make actionlint` (new target, pinned `actionlint v1.7.7`) exits 0 on all three
  workflows and is now part of `ci`, `ci-fast` and `check` — this class of defect must not be
  able to reach the default branch again.
- **T2 done — one build configuration for every checker.** `.golangci.yml` now declares
  `build-tags: [lancedb]` and it is the ONLY declaration: `make lint` dropped its
  `--build-tags` flag (the flag overrode the config, which is how the two came to disagree),
  and both GitHub lint jobs read the config. `make vulncheck` gained a `lancedb-native`
  prerequisite, and the `security` job now runs `make vulncheck` instead of a bare
  `govulncheck ./...`.
- **T3 done — `make lint` exits 0 (was 27 issues).** Details in `## Implementation Details`.
  While fixing it, a 28th surfaced that the first 27 had masked: `internal/ui/node_modules`
  contains a Go package (`flatted`), so after `make ui` the module's `./...` pattern matched
  third-party code and golangci-lint reported a typecheck failure on it. Fixed at the source
  rather than per-tool — see the `ui` target.
- **A real vulnerability was found, and it was invisible.** With the `lancedb` tag,
  `govulncheck` reports GO-2026-6061 in `google.golang.org/grpc@v1.80.0` (xDS RBAC + HTTP/2
  transport), reachable from `internal/ast` through `arrow-go/parquet/pqarrow` →
  `arrow/flight`. Bumped to `v1.82.1`; the scan is now clean. The `security` job could never
  have reported this: the workflow file was rejected, and the job's command lacked the tag.
- **T5 done — CI performance.** Changes, each with its measured or structural justification:
  - `test` job: new cache for `/tmp/onnxruntime-cache` + `/tmp/graphit-model-cache`. It was
    re-downloading the ONNX Runtime tarball and a 132 MB model on every run; `build-check`
    already cached the runtime, the `test` job did not.
  - `build-check`: **added** Rust toolchain + cargo/lancedb caches. `make build-linux` depends
    on `lancedb-native`, and with no installed runtime to link against the target falls through
    to `fetch-lancedb`, which git-clones lancedb-go and runs `cargo build --release`. The job
    that actually builds was the only one paying that from scratch every run.
  - `build-check`: **removed** the Zig 0.16.0 download. Nothing consumed it — no `CC`/`CXX`
    points at it and the build is native amd64. Already recorded as debt in
    `docs/changelogs/20260728_make_ci_and_make_install_functional.md`.
  - `lint` and `security` jobs: added the `libicu-dev pkg-config` step the `test` and
    `build-check` jobs already had. Both build the Rust native, whose `-sys` crates look for a
    C toolchain and pkg-config; three jobs with three different system states is how one fails
    for a reason unrelated to the code it checks.
  - Every third-party tool is now pinned: `govulncheck v1.7.0` and `actionlint v1.7.7` in the
    Makefile, `gocover-cobertura v1.5.0` in the workflow, all invoked with `go run` so they
    stay out of the module graph and come from the build cache. `@latest` meant the pipeline
    could turn red with no commit to explain it, and resolved over the network every run.
  - `ui-build` now calls `make ui-lint` instead of the raw npm command, so it cannot drift.
- **T6 done — the release CI gate no longer diverges from `ci.yml`.** It runs `make vet`,
  golangci-lint (config tags), the new `make actionlint`, `make vulncheck`, `make test`,
  `make ui`, `make ui-lint`, and it now has the Rust toolchain and both caches, with keys
  **identical** to `ci.yml`'s so the two workflows share one cache entry. It gates every
  release artifact and was the slower, more fragile of the two.

### 2026-09-01 — T4 BLOCKED: another agent session is editing this working tree

`make test` cannot be brought green from this session, and the reason is not in the code
this task touched.

**What happened.** The first `make test` of the session passed `internal/ast`
(`ok … 231.173s`) and failed one test elsewhere:
`internal/hub/adapters/ide` → `TestMandatePreambleReAppliesAfterAnInterruption`, asserting the
mandate preamble contains `re-open the skill for the domain you are re-entering`. The
committed `AGENTS.md:12` carries that clause and `mandate.go` did not, so the generated text
had regressed behind the file it generates. Fixed in `mandate.go`; that package is green.

Twenty minutes later a second `make test` failed in `internal/ast` on **three tests that did
not exist when this session started**:

- `TestSkillQueryTemplatesAreAcceptedByTheCanonicalPlanner`
- `TestSkillQueryTemplatesAlwaysNameTheRelationshipType`
- `TestASTRuleContentRunnableQueriesUseRealProperties`

**What they are.** `git status` shows files this task never touched modified —
`internal/ast/rule.go`, `internal/ast/ladybug_icebug_canonical.go` — plus untracked new test
files (`internal/ast/rule_query_templates_test.go`, `internal/ast/dump_skill_queries_test.go`,
`internal/hub/adapters/ide/mandate_preamble_propagation_test.go`), a **staged** change in
`internal/hub/adapters/ide/mandate.go`, and a second task log:
`docs/tasks/generated-instructions-drift-preamble-and-canonical-query-templates.md`.

`internal/ast/rule.go` had been written less than three minutes before that `git status`.
Another agent session is working in this same directory, on that other task, right now. Its
new tests assert that every Cypher template in the generated `graphit-ast` skill is accepted
by the canonical planner; `rule.go` is mid-fix, so they fail. That work is that session's, and
overlaps `mandate.go` with this one.

**Consequence for this task.** Every remaining failure in the suite is in `internal/ast` and
all three are about the AST skill text. Nothing this task changed fails. But a test run taken
while another process is writing the tree measures nothing, and editing `internal/ast/rule.go`
from here would collide with work in progress — so T4 stops at "everything except the
concurrent session's three tests is green", pending the user's decision on sequencing.

**Timing measured (this machine, 20 cores; `internal/ast` alone is 107–126 s of the total):**

| run | exit | wall |
|---|---|---|
| `make test` — cold build cache | 2 (1 test, since fixed) | 406 s |
| `make test GO_TEST_P=8` — warm | 2 (concurrent session's tests) | 137 s |
| `make test GO_TEST_P=4` — warm | 2 (concurrent session's tests) | 143 s |
| `make test-short` — warm | 2 (concurrent session's tests) | 125 s |

**What that says, and it corrects an assumption made when `GO_TEST_P` was introduced:** on a
warm build cache `-p` barely matters — 137 s vs 143 s — because a single package,
`internal/ast`, is 121 s of the run and `-p` cannot split one package. The 406 s → 137 s drop
is the **build cache**, not parallelism. `GO_TEST_P` still earns its place on a cold cache,
where the 1.29 M lines of generated ANTLR compile twice, and it is where a 20-core machine was
previously held at 4; but it is not the lever, and CI gets the real one from `setup-go`'s
cache (keyed on `go.sum`, so ANTLR compiles once per dependency change rather than once per
run).

### 2026-09-01 (resumed) — T4 and T7 done, `make ci` green end to end

The concurrent session finished and committed its work (`648fa1c`, `0c63204`, `ba4bb0d`), so
the block described in the previous entry is gone. On resuming, the skills were re-opened and
every check re-run against the new HEAD rather than trusted from the pre-interruption state —
which mattered: `648fa1c` rewrote `mandatePreamble()` and the assertions in
`mandate_resume_test.go` together, so the one-line preamble fix made earlier in this session
is obsolete and correctly absent from HEAD. That package is green on its own text.

**T4 — `make test` green.** Confirmed as part of the full run below; no `FAIL` line, and
`coverage.out` is produced.

**T7 — verified, on the current HEAD:**

| target | exit | wall | note |
|---|---|---|---|
| `make actionlint` | 0 | <1 s | all three workflow files |
| `make vet` | 0 | 3.8 s | warm |
| `make lint` | 0 | 15 s warm / 3 m 26 s cold | **was exit 2 with 27 issues** |
| `make ui-lint` | 0 | 4.7 s | 3 warnings, 0 errors |
| `make vulncheck` | 0 | ~130 s | **was 1 reachable vulnerability** |
| `make test` | 0 | 137 s warm / 406 s cold | |
| **`make ci`** | **0** | **167 s** | prints `✅ All CI checks passed.` |

`make ci` at 167 s while `test` alone is 137 s and `vulncheck` alone ~130 s is the
parallelisation doing its job: the four static checks overlap the network-bound scan, and only
`test` is serial after them.

Cache paths in the workflow were verified against what the Makefile actually creates:
`/tmp/graphit-model-cache` (133 MB, the embedding model) and `/tmp/onnxruntime-cache` (32 MB)
both exist locally after a test run, which is what the new `test`-job cache keys point at.
`/tmp/lancedb-native-cache` is absent on this machine because `.native/liblancedb_go.so` is a
symlink into the installed runtime, so `lancedb-native` short-circuits — in CI there is no
installed runtime, the target falls through to `fetch-lancedb`, and that path is what the cargo
caches cover.

## Implementation Details

**T1 — workflow validity.** `.github/workflows/ci.yml`: `permissions` reduced to
`contents: read`. New `actionlint` target in the Makefile using a pinned
`github.com/rhysd/actionlint/cmd/actionlint@v1.7.7` via `go run`, wired into `ci`, `ci-fast`
and `check`, plus a "Validate workflow files" step in both the `ci.yml` lint job and the
`release.yml` gate.

**T2 — one build configuration.** `.golangci.yml`: `run.build-tags` `fts5` → `lancedb`, with
the reasoning inline. `Makefile`: `lint` dropped `--build-tags "$(LOCAL_TAGS)"` so the config
is the single declaration; `vulncheck` gained a `lancedb-native` prerequisite. `ci.yml`: the
`security` job runs `make vulncheck`.

**T3 — the 27 lint issues, each on its merits:**

| issue | site | resolution |
|---|---|---|
| `unused` ×17 | `internal/ast/rebuild_index.go` | Deleted `schemaInfo` and the 16 orphaned `*JSON` wrappers. `a216481` ("the icebug export streams instead of materializing") replaced them with `stream*` producers; `collectRows` stays for the callers (tests, delta probe) that genuinely want a whole table. Verified against the graph — zero inbound edges — before deleting. The one comment carrying real knowledge, on `fieldAccessEdgeJSON`'s source-label parameter, was moved onto `streamFieldAccessEdges`. |
| `unused` ×2 | `internal/ast/direct_icebug.go` | `nodeRowsFor` (same orphaned-wrapper pattern) and `appendArrowValueDirect` (the materializing Arrow appender the streaming column writer replaced). |
| `unused` ×1 | `internal/ast/icebug_transfer.go` | `firstDDLLine`. |
| `unused` ×1 | `internal/ast/ladybug_icebug_traversal_test.go` | `bundleSchema`; `bundleSchemaBytes`, which is used, stays. |
| `errorlint` ×2 | `internal/ast/pipeline.go:860,924` | `fmt.Errorf("%w; rollback graph: %v", …)` → `%w` twice. Both errors are now unwrappable, which is the point of wrapping the rollback failure at all. |
| `ineffassign` ×1 | `internal/ast/pipeline.go:689` | The first `fileCluster` resolution was overwritten before use. Deleted, with a `NOTE` recording *why* the surviving one must come after `r.pf.Path` is corrected — a parser may report only the basename, and resolving against that silently picks the default cluster. |
| `ineffassign` ×1 | `internal/ast/pipeline.go:825` | `preparedEntries = nil` looked like a memory release and released nothing: the variable is never read past that point, so Go's stack maps already treat it as dead, and under `stagedSearch` the slice is held by `Complete()` where no local assignment reaches it. Removed, with a `NOTE` so it is not reintroduced. |
| `staticcheck` SA4004 ×1 | `internal/ast/node_columns_test.go:60` | `for … { break }` expressing "only the first row decides" → an explicit `rows[0]` check plus an empty guard. |
| `gocritic` dupBranchBody ×1 | `cmd/graphit/commands/runners.go:407` | The two branches of `if isIncremental && len(absPaths) > 0` were byte-for-byte identical. Collapsed, and the dead `isIncremental` computation removed — the distinction reaching the pipeline is `ForceRebuild: reset \|\| reindex`, which it reads for itself. Also removed a duplicated `if err != nil { return err }` that sat *before* `p.EndProgress()` and so defeated the comment explaining why EndProgress has to run on the error path. |

**The 28th, which the other 27 masked.** `internal/ui/node_modules/flatted/golang/pkg/flatted`
is a Go package inside an npm dependency, so after `make ui` the module's `./...` matched it
and golangci-lint failed with `typecheck: no such file or directory` on it. `linters.exclusions.paths`
filters *issues*, not package loading, and `./...` is expanded by the go tool, not the linter.
Fixed once for all four tools: the `ui` target writes a `go.mod` stub into
`internal/ui/node_modules/` after `npm ci`, and `go list` skips any directory declaring its own
module. `go list ./...` drops from 71 packages to 70, none from `node_modules`.

**The vulnerability.** `google.golang.org/grpc` v1.80.0 → v1.82.1 (GO-2026-6061), reached from
`internal/ast` via `arrow-go/parquet/pqarrow` → `arrow/flight`. Indirect, so the diff is four
lines of `go.mod` and the corresponding `go.sum`.

**T5/T6 — performance and the release gate.** Covered in the entry above.

## Use Cases

### UC-01: A push or PR is actually verified by GitHub
- **Actor**: GitHub Actions, on push to `main`/`develop`/`github`/`feature/**` or a PR to
  `main`/`develop`.
- **Preconditions**: `.github/workflows/ci.yml` parses. Before this task it did not, and the
  consequence was silence rather than failure.
- **Main Flow**: six jobs run in parallel — `lint` (actionlint → `make vet` → golangci-lint
  with the `lancedb` tag), `test` (`make test` → Cobertura → Codecov), `platform-semantics`
  (watcher, ignore rules and store resolution on Linux, macOS and Windows), `security`
  (`make vulncheck`), `build-check` (`make build-linux`), `ui-build` (`make ui` +
  `make ui-lint`).
- **Alternative**: `release.yml`'s gate runs the same set on a tag, then fans out to three
  build runners.
- **Error**: any job failing fails the run. A malformed workflow is now caught locally by
  `make actionlint` before it can be pushed.
- **Postconditions**: every check covers the `-tags lancedb` build that actually ships.
- **Affected Files**: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `Makefile`,
  `.golangci.yml`

### UC-02: A developer runs the same gate locally
- **Actor**: developer or agent.
- **Preconditions**: Go, Node, golangci-lint, and either a cargo toolchain or an installed
  runtime to link the LanceDB native from.
- **Main Flow**: `make ci` → `ui` → `actionlint|vet|lint|vulncheck|ui-lint` in parallel →
  `test` → success banner.
- **Alternative**: `make ci-fast` for the PR loop — the static checks in parallel plus
  `test-short`, skipping the model download and the heavy LanceDB gate.
- **Error**: any target's non-zero exit propagates; `test` collects both phases and exits
  non-zero if either failed.
- **Postconditions**: same tags, same tool versions, same package set as the GitHub jobs.
- **Affected Files**: `Makefile`

## Test Cases & Acceptance Criteria

### Feature: the workflow files are valid
Ref: UC-01
#### Scenario: an unknown permission scope is rejected before it is pushed
```gherkin
Given .github/workflows/ci.yml declares "permissions: code-quality: write"
When make actionlint runs
Then it exits non-zero naming the unknown permission scope "code-quality"
  And make ci fails at that target instead of pushing a workflow GitHub will refuse
```
#### Scenario: the current workflows pass
```gherkin
Given the three files in .github/workflows
When make actionlint runs
Then it exits 0 with no diagnostics
```

### Feature: one build configuration for every checker
Ref: UC-01
#### Scenario: lint analyses the lancedb build in both places
```gherkin
Given .golangci.yml declares run.build-tags: [lancedb]
  And make lint passes no --build-tags flag of its own
When golangci-lint runs locally and in the GitHub lint job
Then both analyse the same package set, including files behind //go:build lancedb
```
#### Scenario: govulncheck carries the tag and finds a reachable vulnerability
```gherkin
Given google.golang.org/grpc is pinned at a version with GO-2026-6061
When make vulncheck runs with -tags lancedb
Then it reports the vulnerability as reachable from internal/ast and exits non-zero
When grpc is upgraded to v1.82.1
Then it reports 0 affecting vulnerabilities and exits 0
```

### Feature: node_modules is outside the module
Ref: UC-02
#### Scenario: a Go package inside an npm dependency is not linted
```gherkin
Given make ui has run and internal/ui/node_modules/flatted ships Go sources
When go list ./... is expanded
Then it returns 70 packages and none under node_modules
  And make lint exits 0
```

### Feature: make ci is green and parallel
Ref: UC-02
#### Scenario: the full gate passes
```gherkin
Given a checkout at the current HEAD with warm build caches
When make ci runs
Then it exits 0 and prints "All CI checks passed."
  And total wall time is below the sum of test and vulncheck run serially
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `.github/workflows/ci.yml` | Modified | Invalid `permissions` scope removed; actionlint step; CGO deps on `lint` and `security`; Rust + cargo caches on `security` and `build-check`; ONNX + model cache on `test`; Zig removed; pinned `gocover-cobertura`; `security` and `ui-build` call make targets |
| `.github/workflows/release.yml` | Modified | Gate aligned with `ci.yml`: actionlint, `make vulncheck`, Rust toolchain, and both caches with identical keys |
| `.golangci.yml` | Modified | `run.build-tags` `fts5` → `lancedb`, as the single declaration |
| `Makefile` | Modified | `actionlint` target; `GOVULNCHECK_VERSION`/`ACTIONLINT_VERSION` pins; `GO_TEST_P` derived from core count, capped at 8; `lint` stops overriding the tags; `vulncheck` depends on `lancedb-native`; `ui` writes the `node_modules/go.mod` stub; `ci`/`ci-fast`/`check` include actionlint |
| `go.mod`, `go.sum` | Modified | `google.golang.org/grpc` v1.80.0 → v1.82.1 (GO-2026-6061) |
| `internal/ast/rebuild_index.go` | Modified | 17 unused functions deleted; one load-bearing comment relocated to the streaming producer |
| `internal/ast/direct_icebug.go` | Modified | `nodeRowsFor`, `appendArrowValueDirect` deleted; stale comment reference updated |
| `internal/ast/icebug_transfer.go` | Modified | `firstDDLLine` deleted |
| `internal/ast/pipeline.go` | Modified | 2 × `%w`; dead `fileCluster` assignment and no-op `preparedEntries = nil` removed, each with a NOTE |
| `internal/ast/node_columns.go` | Modified | Comment no longer refers to the deleted appender |
| `internal/ast/node_columns_test.go` | Modified | SA4004 loop → explicit first-row check |
| `internal/ast/ladybug_icebug_traversal_test.go` | Modified | Unused `bundleSchema` helper deleted |
| `cmd/graphit/commands/runners.go` | Modified | Identical if/else collapsed, dead `isIncremental` removed, duplicated early return that defeated `EndProgress` removed |
| `docs/tasks/make-ci-and-github-ci-green-and-fast.md` | Created | This log |

## Trade-offs & Decisions
- **Explicit workflow steps over a composite action.** The Go + native + cache block is
  repeated across six jobs in two files, and a local composite action would make drift
  impossible. Rejected for now: a composite action cannot be executed on this machine, and the
  task is *"make CI work"* — shipping an unverifiable refactor into the file that had just been
  silently broken trades one invisible failure for another. `make actionlint` catches the
  syntax half locally; the duplication is recorded as debt.
- **Dead wrappers deleted, not `//nolint`-ed.** `.golangci.yml`'s own comment reserves
  `//nolint:unused` for false positives. These were genuinely unreachable, and a caller who
  wants a whole table can write `collectRows(ri.streamX)` at the call site.
- **`make test` in CI stays full, not `-short`.** The `-short` path exists (`ci-fast`,
  `test-short`) and skipping the model download would have been the easy speed-up. Caching the
  model achieves it without dropping coverage, which is the better trade for the branch gate.
- **`GO_TEST_P` capped at 8 rather than `nproc`.** The race detector multiplies per-binary
  memory; the cap is what keeps a 20-core machine from swapping. Overridable.
- **`go run <tool>@<pinned>` rather than a `tool` directive in `go.mod`.** Adding govulncheck
  as a module tool was tried and reverted: `go mod tidy` pulled its test dependencies
  (`go-cmdtest`, `packagestest`) into the graph and upgraded `x/net`, `x/tools`, `x/text` as a
  side effect. `go run pkg@version` is equally pinned and equally cached, and touches nothing.

## Technical Debt
- [ ] **The `Dockerfile` added by `ba4bb0d` is built by no CI job.** A 19 KB Dockerfile that
  nothing verifies is the same shape of gap this task just closed elsewhere. A `docker build`
  check is not free — the image installs a published release over the network — so the useful
  form is probably a lint (`hadolint`) on every push plus a real build on tags. Left out
  deliberately: adding an unrequested job to the workflow that had just been repaired is how
  the repair gets blamed.
- [ ] **`npm ci` reports 32 advisories in the UI dependency tree (2 critical), and no CI job
  looks at them.** `make vulncheck` covers Go only. These are build-time dev dependencies, and
  `npm audit fix --force` involves breaking changes, so this needs a decision rather than a
  command.
- [ ] The Go + native + cache setup block is duplicated across six jobs in two workflow files.
  A local composite action would fix it; see the trade-off above for why not now.
- [ ] Carried from `docs/tasks/optimize-make-ci.md`: `internal/ai` and `internal/lancestore`
  heavy tests still use real disk/network instead of an injected `ModelManager`/`Store`.
- [ ] Carried: `vulncheck` at ~130 s is the slowest single target in `make ci`; it is
  network-bound on the vulnerability database and overlaps the other static checks, so it sets
  the floor for the parallel phase.
- [ ] Carried: `BUILD_ID ?= $(shell …)` is recursive, so each expansion re-runs `$(shell)` and
  the three `go build` invocations of `build-linux` get different UUIDs.

## System Knowledge
- **An invalid workflow file is indistinguishable from a passing one at a glance.** GitHub
  rejects the file over one unknown `permissions` key and the repository shows no runs.
  `actionlint` catches it in milliseconds, which is why it is now in `ci`.
- **`fts5` is dead but was still load-bearing in configuration.** `Makefile:43` documents that
  SQLite and its tag are gone; `.golangci.yml` still pinned it. The Makefile comments at `:828`
  and `:852` describe the failure a wrong tag produces: `internal/ast` and `internal/wiki`
  resolve to a build-guard file, so the checker misses every real diagnostic *and* stops on an
  undefined guard symbol — which reads like a broken tool rather than a missing flag.
- **A golangci-lint `--build-tags` flag OVERRIDES `run.build-tags`.** That is the whole
  mechanism by which local and CI lint diverged, and it is invisible in both invocations.
- **`go list ./...` skips any directory that declares its own module.** One `go.mod` stub is a
  complete answer to third-party Go sources appearing inside `node_modules`, and it works for
  every go tool at once — where a grep filter has to be repeated per tool and was missing from
  two of the four.
- **On a warm build cache, `go test -p` barely moves the needle here.** `internal/ast` is
  ~121 s of a ~137 s run and `-p` cannot split one package. The lever is the build cache
  (406 s cold → 137 s warm); in CI `setup-go` provides it, keyed on `go.sum`, so the 1.29 M
  lines of generated ANTLR compile once per dependency change rather than once per run.
- **`ineffassign` on `x = nil` is usually right, and the intent behind the line is usually
  "release memory".** For a local that is never read again, Go's precise stack maps already
  treat it as dead — the assignment frees nothing. Worth a NOTE at the site, because the next
  reader will want to add it back.
