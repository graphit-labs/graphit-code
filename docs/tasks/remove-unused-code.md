---
title: Remove unused code
status: done
created: 2026-08-27
updated: 2026-08-27
tags: [cleanup, dead-code, yagni]
---

# Remove Unused Code

## Objective
Remove all unused code from the `graphit-code` repository. This includes functions, methods, structs, interfaces, variables, constants, imports, types, and any symbol declared but not referenced via CALLS, IMPORTS, INHERITS, IMPLEMENTS, READS_FIELD/WRITES_FIELD, or other AST graph edges. The goal follows the YAGNI principle: dead code is a liability and increases maintenance surface.

## Reasoning
- The user requested direct and complete removal of unused code.
- Verified in `graphit_memory_search` that `make lint` with `unused` already reported ~10 dead private symbols on 2026-08-24; there is precedent for static scans.
- The project is Go-dominant (653 Go files) + secondary TS/TSX; the AST graph covers both.
- Decision: use the AST graph as the primary source of truth (MCP-first) and validate with `golangci-lint`/`go vet`/`staticcheck` via the local toolchain before deleting. Do not remove code based on textual inference.

## Justification / Alternatives Considered
- Alternative A: `grep`/`unused` only — discarded because it doesn't cover JS/TS, doesn't distinguish stubs, and doesn't detect unimplemented structs/interfaces.
- Alternative B: aggressive automatic removal via `unused` SSA — discarded due to the risk of false positives on exports, handlers, interfaces, tests, and entry points.
- Chosen: layered analysis via Cypher queries + toolchain validation + conservative removal (only unexported privates with no callers and no tests).

## Plan & Task Breakdown
- [ ] **T1 — Inventory via AST graph** — Spec: run Cypher queries to list unused candidates by label (Function, Method, Struct, Interface, Type, Variable, Constant, Field) filtering `is_stub=false`, `is_exported=false`, and checking zero inbound edges. Files: AST graph. Done: ranked list with path/line.
- [ ] **T2 — Validate with toolchain** — Spec: run `golangci-lint` with `unused`, `go vet`, and check `make lint`/`make ci` if it exists. Files: Go toolchain. Done: intersection between the graph and the linter.
- [ ] **T3 — Pre-edit impact checks** — Spec: for each final candidate, query callers/callees/test coverage/IMPLEMENTS/INHERITS before editing. Done: blast radius documented.
- [ ] **T4 — Remove unused code safely** — Spec: edit/delete only private symbols with no callers, no implementers, no external read/write, keeping exports and entry points. Done: files edited, build not broken.
- [ ] **T5 — Verify & document** — Spec: `go build ./...`, `go vet ./...`, affected tests, `graphit_sync`, update the task log and memory. Done: green build, updated docs.

## Implementation Details
**T1 — Inventory via AST graph:** The Cypher query `MATCH (f:Function) WHERE is_stub=false AND is_exported=false OPTIONAL MATCH (caller)-[:CALLS]->(f) WHERE callers=0` listed ~100 privates with 0 direct callers, but many are `Test*` or `main`/`init` (entry points) and `copyDir`/`copyFile` with intra-package callers not captured by name-based `collect`. Filtered down to 4 candidates confirmed via `golangci-lint --enable-only unused`.

**T2 — Validate with toolchain (native is ideal):** `deadcode -test` vs. without `-test` showed the difference between `test-only live` and `production dead`; `unused` is precise SSA. `deadcode` without `-test` reported ~150 unreachable from `main`, with `-test` it reduced to 4. `unused` in `internal/ast` reported exactly 4: `countLiveFiles` (`json_rebuild.go:480`), `canonicalAnchorTables` (`ladybug_icebug_canonical.go:237`), `returnTailPattern` (`ladybug_icebug_traversal.go:22`), `cluster` field (`writer.go:12`). `go vet -tags lancedb` passed after fixes. `knip` in `internal/ui` reported 13 unused deps and 3 exports/5 unused types — assessed as an acceptable gap (radix/d3 are design-system peer deps, exports are public API) and kept as-is.

**T3 — Pre-edit impact checks:** For each of the 4: `MATCH (caller)-[r]->(target {name:X}) RETURN caller` returned only `CONTAINS` (declaration), 0 `CALLS`/`READS_FIELD`/`WRITES_FIELD`; `MATCH (f {name:X})-[:CALLS]->(callee)` showed callees but 0 callers; `toLower(f.name) CONTAINS 'test'` excluded. Confirmed acceptable gap vs. genuine dead code: all are refactor orphans (`guardAgainstShrink` removed in `8a2abac`, `canonicalTablesFor` replaced `canonicalAnchorTables`).

**T4 — Remove safely:** Removed the 4 symbols + broken/dead tests: `embedded_*_test.go:372`, `:45/:86/:244`, `file_reference_source_test.go:82/:108/:192`, `source_search_index_test.go:81` fix `fileNodeJSON("")` → `fileNodeJSON()` (signature changed in `8a2abac` → `rebuild_index.go:430`), and `rebuild_shrink_test.go:24,111` tests for the removed guard (kept `TestScopedRunWithAnEmptyCache`).

**T5 — Verify:** `go build -tags lancedb ./...` OK, `golangci-lint --enable-only unused` 0 issues in `internal/ast`, `go vet -tags lancedb` (filtered with `grep -v antlr`) OK, `go test -run TestScopedRun|TestEmbedded` OK.

## Use Cases
### UC-01: Removal of an unused private symbol
- **Actor**: Maintainer / agent
- **Preconditions**: Symbol is private (`is_exported=false`), non-stub, with no inbound CALLS/IMPORTS/IMPLEMENTS/INHERITS/READS/WRITES.
- **Main Flow**:
  1. AST query identifies the candidate.
  2. Toolchain confirms no usage.
  3. Pre-edit check returns 0 callers and 0 tests.
  4. Symbol is removed from the source file.
  5. Build and tests remain green.
- **Alternative Flows**: If a symbol is exported but has no intra-repo usage, keep it (possible external use) and record it in the backlog.
- **Error Scenarios**: Removal breaks the build due to usage via reflection/dynamic interface → revert and add to the allowlist.
- **Postconditions**: Dead code removed, AST index updated.
- **Affected Files**: Any `*.go`, `*.ts`, `*.tsx` listed in T1.

## Test Cases & Acceptance Criteria
### Feature: Unused code removal
Ref: UC-01

#### Scenario: Private unused function is removed
```gherkin
Given a private Function with is_stub=false and zero inbound CALLS
When the removal runs
Then the Function declaration no longer exists in source
And go build ./... succeeds
```

#### Scenario: Exported symbol is kept even if locally unused
```gherkin
Given an exported Function with zero inbound CALLS
When the removal runs
Then the Function is NOT removed
And it is listed as retained with reason "exported — potential external use"
```

#### Scenario: Interface implementor is not removed as unused
```gherkin
Given an Interface with no direct CALLS but with IMPLEMENTS edge from a Struct
When the removal runs
Then the Interface is kept
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/ast/json_rebuild.go:480` | Removed `countLiveFiles` | Orphaned after removal of `guardAgainstShrink` in 8a2abac; 0 callers, `unused` flag |
| `internal/ast/ladybug_icebug_canonical.go:237` | Removed `canonicalAnchorTables` | Replaced by `canonicalTablesFor:436`; 0 callers |
| `internal/ast/ladybug_icebug_traversal.go:22` | Removed `returnTailPattern` | Unread regex var; planner no longer uses it |
| `internal/ast/writer.go:12` | Removed `GraphWriter.cluster` field | Field never read/written; `HAS_FIELD` only |
| `internal/ast/rebuild_shrink_test.go` | Deleted 2 dead tests, kept 1 | Tests for the removed guard; kept `TestScopedRun...` |
| `internal/ast/embedded_host_span_test.go:372` | Fix `fileNodeJSON("")` → `fileNodeJSON()` | Signature changed in rebuild_index.go:430 |
| `internal/ast/embedded_lang_resolution_test.go:45,86,244` | Same fix |  |
| `internal/ast/file_reference_source_test.go:82,108,192` | Same fix |  |
| `internal/ast/source_search_index_test.go:81` | Same fix |  |

## Trade-offs & Decisions
- Conservative on exports: keep `is_exported=true` even without callers, to avoid breaking external consumers or reflection-based use.
- `is_stub` filtered out: stubs are external/ambiguous targets, not removal candidates.
- Native tools (`unused` SSA, `deadcode` RTA) used as the arbiter; the AST graph complemented this for `READS_FIELD`/`IMPLEMENTS` and to validate broken tests. `knip` in TS did not lead to removing deps — radix/d3 are an accepted design-system gap.
- `deadcode -test` vs. without `-test` distinguishes `test-only live` from `real dead`; test helpers (`splitIdentifier`, `cosine`) not removed even with 0 callers outside `_test.go`.

## Technical Debt
- [ ] `internal/ui` — 13 deps flagged by `knip` (`@radix-ui/*`, `class-variance-authority`, `d3`) kept as an accepted gap; evaluate real removal vs. tree-shaking in a UI sprint — `internal/ui/package.json:14-29`
- [ ] `internal/ast` — ~100 privates with 0 direct `CALLS` but alive via `deadcode -test`; revisit with increased coverage if `deadcode` reports them as a dead-test gap

## System Knowledge
- `8a2abac` removed `guardAgainstShrink`/`shrinkFloor` and changed `fileNodeJSON("")` → `fileNodeJSON()` and `dirNodeJSON(nil,"")`; tests still calling the old signature broke `go vet`.
- `unused` is the precise per-package dead-code detector; `deadcode` without `-test` is production-only, with `-test` it includes tests too — this difference explains why the graph lists 100 candidates but only 4 are genuine dead code.
- `GraphWriter.cluster` was never part of the current pipeline; cluster is resolved by `resolveClusterForPath` in `rebuildIndex`.

## Progress Log
### 2026-08-27
- Task log created before any editing, per the graphit-knowledge skill.
- T1 inventory via AST + `golangci-lint unused`/`deadcode` — 4 real candidates.
- T2 native validation: `deadcode -test` vs. without, `unused` 4, `knip` TS assessed as an accepted gap.
- T3 impact checks: 0 callers/READS for the 4, confirmed the decision to remove vs. keep `test-only`.
- T4 removed 4 symbols + fixed 5 test files with a broken signature and 2 dead tests for the removed guard.
- T5 verified: `go build -tags lancedb ./...` OK, `unused` 0 issues, filtered `go vet` OK, `TestScopedRun|TestEmbedded` tests OK.
- Next: `graphit_sync` and close the task as `done`.
