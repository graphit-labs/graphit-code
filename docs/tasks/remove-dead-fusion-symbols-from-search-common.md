---
title: Remove the dead Go-side fusion and trigram symbols from search_common.go
status: in-progress
created: 2026-08-24
updated: 2026-08-24
tags: [ast, search, cleanup, lint]
---

# Remove the dead Go-side fusion and trigram symbols from `search_common.go`

## Objective

`golangci-lint run --build-tags lancedb ./internal/ast` reported four unused symbols, all in
`internal/ast/search_common.go`:

```
search_common.go:21:7:  const rrfK is unused
search_common.go:209:6: func identifierTrigrams is unused
search_common.go:223:6: func queryTrigrams is unused
search_common.go:304:6: func indexedText is unused
```

They are residue of the migration that gave ranking back to the engine — the same cleanup
that deleted `internal/ast/search_fusion.go`. `rrfK` was the Go-side Reciprocal Rank Fusion
constant; `identifierTrigrams`/`queryTrigrams` were the hand-rolled trigram bag;
`indexedText` was the document-text normaliser. The engine does all three now.

Surfaced while working on an unrelated task
([global dir override](global-dir-override-by-env-var.md)), deferred to the backlog there,
and picked up on the Engineer's instruction in the same session.

## Pre-edit impact check

Run against the graph before deleting anything:

| Symbol | Inbound references |
|---|---|
| `rrfK` | none — a constant with no reader anywhere in the tree |
| `identifierTrigrams` | one, from `queryTrigrams`, which is itself dead — a closed cluster |
| `queryTrigrams` | none |
| `indexedText` | none |

Two neighbours were checked explicitly because deleting a caller can strand its callee:

- `normalizeForTrigrams` **survives** — `identifierGrams` (`search_lance.go:241`) still
  calls it. It is the gram alphabet, and the engine-side path uses it.
- `splitCodeIdentifier` **survives** — `entityBody`, `fileBody`, `tokenizeQuery` and a test
  all call it.

So the removal does not cascade. Verified afterwards that no reference to any of the four
remains in any `.go` file, under any build tag.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/search_common.go` | Modified | Removed `rrfK`, `identifierTrigrams`, `queryTrigrams`, `indexedText` and their doc comments; corrected the file header, which still claimed fusion lived here |
| `internal/ast/search_lance.go` | Modified | Two comments referred to the deleted functions by name — `identifierGrams is identifierTrigrams widened…` and `the same pairing tokenizeQuery/queryTrigrams had` — and were rewritten to stand on their own |
| `internal/ast/grammar_archive_test.go` | Modified | Removed the unused `testPythonGrammarSOPath` member found by the grouped-constant audit |
| `internal/ladybugstore/icebug.go` | Modified | Removed obsolete `icebugBatchRows` and migrated Arrow's deprecated `NewRecord` to `NewRecordBatch` |
| `docs/guides/retrieval_architecture.md` | Modified | Preserved the measured semantic-weight result outside source code |
| `internal/wiki/search_text.go` | Modified | Removed the unused FTS5-era tokenizer/stopword cluster while retaining the live snippet helpers |
| `internal/wiki/values.go` | Deleted | All four SQLite-era row/vector conversion helpers were unreferenced |
| `internal/ladybugstore/icebug_alternatives_test.go` | Modified | Removed the unused `evenEdges` test helper |
| `internal/hub/s3_store_test.go` | Modified | Expressed the chronological ordering assertion directly as `early >= late` |
| `internal/wiki/crossref.go` | Modified | Applied staticcheck's unconditional `TrimPrefix` simplification |
| `internal/hub/icebug_mount_test.go` | Modified | Asserted pipeline write, discovery and parse results before attempting to publish a missing graph |
| `internal/hub/main_test.go` | Modified | Removed the redundant post-init HOME replacement that made runtime grammar reloads follow an empty directory |

## Key Decisions

- **The comments went with the code, except where they explained something still true.**
  The header of `search_common.go` listed "how passes are fused" among what the file owns;
  that stopped being true when fusion moved to the engine, so it now says so explicitly
  rather than being quietly trimmed. A reader who wonders where the fusion went is the
  reader this file has to answer.
- **`search_lance.go` was in scope even though the linter never named it.** A comment that
  defines a function in terms of a function that no longer exists is a dangling reference —
  the same defect as dead code, one indirection out. Both were rewritten to describe what
  the surviving code does.
- **`weightSemantic` was removed only after its measurement moved to the retrieval guide.**
  The constant had no reader because fusion belongs to the engine; keeping its rationale in
  source would make a deleted implementation detail look configurable.
- **The grouped-constant audit used Go syntax, not linter output.** The AST graph records
  constant declarations but not reads, while `unused` can treat a live member as evidence for
  the whole group. A parser-level identifier census found the remaining candidates without
  scanning comments or string literals as false references.

## System Knowledge

**`golangci-lint`'s `unused` has a blind spot for grouped `const` declarations.** When one
member is used, the group can be treated as used — useful for enums, but able to hide an
independent dead member. The audit found `weightSemantic`, `testPythonGrammarSOPath`, and
`icebugBatchRows`; all three were removed. Exported constants and enum arms were deliberately
left alone because a repository-local zero-reference count cannot prove their public or
generated use is dead.

## Notes

- Historical mentions in `docs/changelogs/20260726_*.md` and in
  `docs/tasks/disable-grammars-by-config.md` were left alone. They record what the code was
  at the time, which is what a changelog and a closed task log are for.
- Backlog item `quatro-simbolos-mortos-em-internal-ast-search-common-go-sobr` was removed as
  resolved; a narrower one was opened for `weightSemantic`.
- The narrower `weightSemantic` backlog item was resolved during the Engineer-requested
  continuation and removed after verification.

## Verification

- `golangci-lint run --build-tags lancedb ./internal/ast` → **0 issues** (was 4).
- `make lint` (whole repo, `./...`) → clean.
- `make test` → 47 packages ok, 0 FAIL. `TestSearchIndexQualityFloor` unchanged at 11/11
  strict and 5/5 recall, which is the signal that nothing removed was still feeding search.

## Resumption Plan

- [x] **T1 — Resolve the remaining `weightSemantic` backlog item** — Spec: preserve the
  measured ranking rationale in `docs/guides/retrieval_architecture.md`, remove the dead
  constant and narrow the source comment to the live cosine floor.
- [x] **T2 — Audit grouped Go constants** — Spec: inspect grouped `const` declarations for
  other unexported members hidden from `unused`; do not treat enum members or build-tagged
  declarations as dead without concrete reference evidence.
- [x] **T3 — Verify and close** — Spec: run focused tests plus repository lint/test gates,
  remove the resolved backlog item, update this log, and leave the complete result staged.
- [x] **T4 — Clear the remaining repository lint findings** — Spec: resolve the ten dead
  symbols and three staticcheck findings surfaced by `make lint`, with graph impact checks
  and without suppressing valid diagnostics. The Apache Arrow deprecation must be grounded
  through Hub or official dependency documentation before changing the call.
- [x] **T5 — Make the full-suite pipeline failure actionable** — Spec: the Hub round-trip test
  must inspect `PipelineResult.WriteErrorCount`, because `RunPipeline` reports rebuild failures
  there while returning a nil top-level error. Done when package-concurrent failures identify
  their actual write cause rather than surfacing later as a missing graph.

## Resumption Progress Log

### 2026-08-24

- Resumed after the Engineer asked to finish both the dead-symbol task and its remaining
  `weightSemantic` backlog item.
- Re-read the task records and the decision memory, then synchronized all indexes because
  the staged implementation came from another agent.
- The AST impact check reconfirmed that `weightSemantic` has no inbound relationship beyond
  file containment. The graph also showed that Go constant reads are not represented: the
  live `semanticFloorCosine` has the same containment-only shape, so grouped-constant liveness
  must be verified textually.
- Hybrid entity discovery hit the known score-column incompatibility (`_distance` expected,
  `_score` returned); FTS discovery and exact Cypher queries remained available and were used.
- A Go-parser audit of every non-generated grouped package `const` found two additional
  unexported zero-reference members: `testPythonGrammarSOPath` in
  `internal/ast/grammar_archive_test.go` and `icebugBatchRows` in
  `internal/ladybugstore/icebug.go`. The latter is residue from the rejected batched Parquet
  writer; the one-row-group invariant now requires one write per table and is protected by
  `TestIcebugWritesOneRowGroupPerFile`.
- T1 and T2 landed: the semantic-weight measurement now lives in the retrieval guide;
  `weightSemantic`, `testPythonGrammarSOPath`, and `icebugBatchRows` were removed, and the
  cosine-floor comment now describes only the live behavior.
- Focused verification exposed an unrelated staged inconsistency before any lint gate ran:
  `internal/ast/search_hybrid_floor_test.go:168` calls undefined `hasName`. The
  `internal/ladybugstore` tests passed. Investigation of the missing helper is the next step;
  no workaround or suppression has been applied.
- Investigation corrected that interpretation: `hasName` exists in
  `internal/ast/search_lance_test.go`, which is selected by the repository's `lancedb` test
  tag. The untagged ad-hoc command was not the project gate; verification continues with
  `-tags lancedb` and the Makefile targets. No production or test helper change is warranted.
- Tagged focused tests passed (`internal/ast` in 177.340s; `internal/ladybugstore` cached).
- `make lint` then reported 13 remaining findings: three staticcheck simplifications/API
  warnings plus ten dead symbols in `internal/wiki`, `internal/ladybugstore`, and one Hub
  test. T4 was added so the lint gate can be made genuinely clean rather than narrowing the
  command to the originally reported package.
- T4 landed: the two staticcheck simplifications were applied; Arrow v18's local API docs
  confirmed `NewRecordBatch() arrow.RecordBatch` as the exact replacement accepted by
  `pqarrow.FileWriter.Write`; and the ten reported private dead symbols were removed. The
  wiki cleanup deletes only the obsolete tokenizer/value-conversion clusters and retains the
  live snippet path.
- Full `make test` passed every package changed in this continuation, including
  `internal/ast`, `internal/ladybugstore`, and `internal/wiki`, but failed one Hub test:
  `TestIcebugArtifactMountsAndAnswers` could not find the graph fixture at its temporary
  `store/ladybugdb` path during publish. This failure is being reproduced in isolation before
  deciding whether it is a regression, a race-only fixture problem, or environmental.
- The exact failing Hub test passed in isolation with the same `-race -tags lancedb` settings,
  publishing seven Parquet files, mounting five nodes, serving the source index, and traversing
  one `CALLS` edge. The first failure is therefore not reproducible and does not intersect the
  changed paths; the Hub package and then the full gate will be rerun before closure.
- The Hub package passed as a whole in isolation, but the second full `make test` reproduced
  the missing graph at the same point. The discriminator is now package-level concurrency
  under `go test ./...`, not randomness inside `internal/hub`. Investigation is narrowed to
  shared process/global resources used by `RunPipeline` and cross-package test setup.
- The pipeline contract explains the misleading symptom: rebuild failures increment
  `PipelineResult.WriteErrorCount` and populate `WriteErrorFiles`, but
  `TestIcebugArtifactMountsAndAnswers` discarded the result and checked only the top-level
  error. T5 will assert that result immediately so the next concurrent run exposes the real
  writer failure.
- T5 landed: `TestIcebugArtifactMountsAndAnswers` now captures `PipelineResult` and fails
  immediately with `WriteErrorCount` plus `WriteErrorFiles`, preserving the pipeline contract
  and making concurrency-only write failures diagnosable.
- The next full run reported zero write failures and still produced no graph. T5 was tightened
  to assert the fixture discovery/parse counts as well; this distinguishes an unavailable Go
  grammar from a graph writer failure before the publish step obscures either cause.
- The tightened assertion identified `TotalFiles:0`, while the reload log named an empty
  `/tmp/graphit-hub-test-home-*/.graphit/runtime/dev/ast/queries`. Root cause: Hub's `TestMain`
  replaced `HOME` after `internal/testsupport` had seeded the runtime queries during package
  initialization. The periodic query reload eventually followed the second empty home and
  erased the extension registry; isolated runs finished before that interval, which explained
  the concurrency-shaped flake. Removed the redundant second HOME isolation: `internal/brand`
  already creates a per-test-binary home before package initialization and also isolates git
  configuration and identity.
- Verification closed cleanly: the focused Hub round-trip passed with the race detector,
  `make lint` reported 0 issues, and `make test` passed both the race-enabled project packages
  and the appended generated-parser packages. The resolved `weightSemantic` backlog item was
  removed through the improvements registry.
- The Engineer then requested that the complete staged result be committed directly on
  `main`. Before committing, the branch, index contents and absence of unstaged tracked
  changes are rechecked; the local `.graphit/` runtime directory remains intentionally
  untracked and outside the commit.
