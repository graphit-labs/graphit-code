---
title: Diagnose post-publication graph-write latency
status: in-progress
created: 2026-08-31
updated: 2026-08-31
tags: [ast, performance, icebug, profiling]
---

# Diagnose post-publication graph-write latency

## Objective

Explain why the AST indexing command can remain for minutes in the user-visible graph-writing phase after the Ladybug/Icebug directory has already been fully produced and published. Establish the current execution path, measure every meaningful phase on a representative corpus, and identify any work that is mislabeled, duplicated, serialized, or performed after publication without visibility.

The investigation must use the current architecture, not the superseded file-backed LadybugDB copy/swap path. Existing project knowledge says the current local graph is exported directly to Icebug/Parquet, mounted in memory for queries, and published by directory rename; relationship and node-table writes were parallelized on 2026-08-31. Therefore the first hypothesis is an observability boundary gap: the broad `write` timer may include shard decoding, direct export, search-index work, verification, cleanup, or publication-adjacent work that the progress UI still reports as “writing graph.” The alternative hypothesis is a real remaining serialized or repeated pass after the visible directory appears complete.

## Plan & Task Breakdown

- [x] **T1 — Map the current write/publish call path** — Spec: use the AST graph to identify the indexing entry point, every callee that contributes to the write timer, and the exact point where the Icebug directory becomes visible; done when the call chain and timer boundaries are explicit, without relying on historical copy/swap assumptions.
- [x] **T2 — Audit existing timing and progress instrumentation** — Spec: read only the relevant indexed source entities and tests; done when every emitted phase label is mapped to the operations it includes and any uninstrumented interval is named.
- [x] **T3 — Reproduce and profile phase timings** — Spec: run the current binary/tests on a representative local corpus with the daemon state controlled, capture wall time/CPU/RSS and internal phase timings, and repeat enough times to distinguish fixed cost from corpus-sized work; done when the dominant interval is measured rather than inferred.
- [x] **T4 — Isolate the bottleneck** — Spec: use targeted probes or existing tests to split the dominant interval into shard load/decode, node collection, node Parquet writes, relationship scans/writes, search-index update, publish/rename, verification, and cleanup; temporary probes must be removed unless they become production instrumentation.
- [x] **T5 — Report findings and next action** — Spec: update this log with the root cause, evidence, affected entities, and acceptance criteria for a future fix; if a safe instrumentation-only change is clearly necessary, request or infer authorization only within the user’s stated “time every phase” scope, then test it proportionally.

## Implementation Details

The pipeline now records cache-save, graph prepare/export/publish, embedding-cache load, search open/build/maintenance/close, and exposes them through `PipelineResult.WritePhases`. CLI progress switches from “Writing graph” to “Building search index” immediately after the Icebug rename and then to “Maintaining search index”; the final timing report prints the write breakdown. The exported rebuild API remains backward compatible: a private timed helper carries the new measurements while existing wrappers still return only `error`.

The temporary real-store probe was removed after collecting evidence. It recreated graph and search artifacts only under test temporary directories and never mutated the published store.

The root cause is an observability boundary error, with a real downstream performance hotspot. `runFileWorkerPool` published the Icebug graph and then rebuilt and maintained the LanceDB search sidecar while the CLI continued to show the same “Writing graph” label and kept all of that work inside `WriteTime`. Publication itself is effectively free. On the final end-to-end run built from the changed source, graph preparation took 0.61s, graph export 0.32s, publication rounded to 0.00s, embeddings 0.20s, search build 26.62s, search maintenance 0.57s, and the broad write interval was 28.33s. The detailed probe attributed 22.914s of a 25.667s search rebuild (89.3%) to creation of the entity IVF-PQ vector index.

## Use Cases

### UC-01: Diagnose a long graph-writing phase
- **Actor**: Graphit engineer running AST indexing on a large repository.
- **Preconditions**: The current Graphit binary and a representative indexed corpus are available; the daemon’s activity is known before measurement.
- **Main Flow**:
  1. Run indexing with phase timing enabled.
  2. Correlate the user-visible “writing graph” phase with current code boundaries.
  3. Split the interval into measurable subphases.
  4. Identify the largest wall-time contributor and whether it occurs before or after publication.
- **Alternative Flows**:
  - Use an existing cost probe when a full large-corpus rebuild would be unnecessarily expensive.
  - Use a smaller corpus to quantify fixed per-table/per-phase overhead before validating the result on the representative corpus.
- **Error Scenarios**:
  - A concurrent daemon rebuild contaminates CPU or I/O measurements; discard that run and repeat with the interference controlled.
  - Existing logs suppress the needed breakdown; use a temporary focused probe and remove it after collecting evidence.
- **Postconditions**: The unexplained interval is assigned to concrete operations with measured durations.
- **Affected Files**: `internal/ast/pipeline.go`, `internal/ast/direct_icebug.go`, and any timing/progress caller identified through the AST graph.

## Test Cases & Acceptance Criteria

### Feature: Graph-write performance diagnosis
Ref: UC-01

#### Scenario: Every major indexing phase has a measured duration

```gherkin
Given the current AST indexing pipeline and a representative corpus
  And the daemon state is known before the run
When a full or forced rebuild completes
Then discovery, hashing, parsing, shard loading, graph export, search indexing, publication, verification, and cleanup are each timed or explicitly proven absent
  And the sum of measured intervals explains the observed wall time within normal instrumentation overhead
```

#### Scenario: Historical copy/swap behavior is not mistaken for current behavior

```gherkin
Given the local graph uses direct Icebug/Parquet export and an in-memory Ladybug catalog
When the post-publication delay is analyzed
Then no conclusion attributes it to the removed LadybugDB Shutdown/Close path
  And the current call graph and source establish the actual work being performed
```

#### Scenario: Contaminated measurements are rejected

```gherkin
Given another indexing process or daemon rebuild consumes CPU or disk during a timing run
When the measurements are reviewed
Then that run is marked contaminated and excluded
  And the measurement is repeated under controlled conditions
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/diagnose-post-swap-graph-write-latency.md` | Created | Open the investigation with current architecture, measurable tasks, and handoff-ready acceptance criteria. |
| `internal/ast/icebug_rebuild.go` | Modified | Split graph preparation, direct export, and publication timing without changing the public rebuild signatures. |
| `internal/ast/pipeline.go` | Modified | Record write subphases and emit progress transitions after graph publication. |
| `cmd/graphit/commands/runners.go` | Modified | Display cache, graph, search-build, and maintenance phases plus the timing breakdown. |
| `cmd/graphit/commands/index_progress_test.go` | Modified | Prove every post-parse phase has a distinct user-visible label. |
| `internal/ast/search_index_missing_test.go` | Modified | Prove graph/search timings are populated for full and search-only rebuilds. |

## Trade-offs & Decisions

- Treat the old Shutdown/Close result as historical evidence only. The architecture that produced it was removed, so repeating that hypothesis would measure the wrong pipeline.
- Start with structural and existing instrumentation evidence before adding probes. A probe is justified only for intervals the current timers cannot separate.
- Do not optimize during diagnosis unless measurement establishes the bottleneck and the requested phase-timing scope clearly covers the minimal observability change.

## Technical Debt

- [x] Confirm whether the user-visible “writing graph” label spans search indexing, verification, or cleanup after the Icebug directory is published — it spans search rebuild, maintenance, and close; the progress labels now split those phases.
- [x] Confirm whether the recently parallelized export still has an O(corpus) repeated-read or collection interval that is not separately reported — shard decode was 0.60–1.29s and graph export 0.33–0.90s on the measured 848/849-file store; both are now separately reported.
- [ ] Investigate why the final forced rebuild discovered/parsed 848 files but reported 854 live cached shards in cache-save, graph, and search phases. This is tracked separately in the improvement backlog as “Force rebuild discovered 848 files but exported 854 cached shards”; do not conflate it with the measured IVF-PQ bottleneck.

## System Knowledge

- The 2026-08-19 measurement that blamed `Shutdown+Close` belongs to the removed file-backed copy/swap architecture and is superseded for the current local pipeline.
- Current project documentation says direct Icebug/Parquet export is the graph build path; both relationship writes and the pure write side of node tables were parallelized on 2026-08-31, while sequential node collection and full-corpus shard residency remain.
- `runFileWorkerPool` starts its broad `WriteTime` before Icebug rebuild but stops it only after LanceDB search rebuild/update, maintenance, and close. The progress reporter previously received no phase transition after graph publication.
- On the valid 849-file probe, search rebuild was 25.667s of a 27.44s post-parse path. Inside it, IVF-PQ vector-index creation was 22.914s (89.3% of search rebuild), row append was 1.735s, entity text indexing 0.958s, and every other search-index creation step together was under 60ms.

## Progress Log

### 2026-08-31

- Searched project memory and documentation before source inspection.
- Corrected a stale important-memory mirror that still presented Shutdown/Close as the current bottleneck.
- Confirmed the daemon is running; no current dream report contains a prior decision blocking this investigation.
- Opened this task log before source or runtime profiling.
- Mapped the broad write timer: it begins before Icebug rebuild and ends only after LanceDB search rebuild/update, index maintenance, and close.
- Added an opt-in real-store probe that writes graph and search artifacts only to test temporary directories.
- First probe build was rejected because the default Go test build excludes LanceDB; no measurement was produced. The next run uses the project-required `-tags lancedb` build tag.
- The first tagged run resolved a test-process store with zero shards, so its sub-millisecond numbers are invalid and excluded. The probe now prints the resolved store and accepts an explicit `GRAPHIT_WRITE_PHASE_PROBE_STORE` override to target the exact production cache.
- Valid real-store probe (848 files, 30 node tables, 238,290 edges): shard decode 1.293s; graph export 0.902s; graph publish/rename 43µs; embedding-cache load 0.566s; search open 0.002s; search rebuild 41.664s; search maintenance 0.596s; search close 0.001s. The broad post-publication delay is therefore inside the search rebuild, not graph publication. The probe is being extended to time row appends and each search-index kind independently.
- Detailed repeat (849 files, 238,655 edges): shard decode 0.598s; graph export 0.329s; publish 24µs; embedding-cache load 0.217s; search rebuild 25.667s; maintenance 0.543s. Search breakdown: append rows 1.735s; files FTS 0.029s; files B-tree 0.003s; entity FTS 0.958s; entity IVF-PQ 22.914s; entity bitmap 0.012s; entity B-tree 0.014s.
- Removed the temporary probe after evidence collection and implemented production timing/progress fields so future runs expose the gap directly.
- Targeted progress-reporter tests and search-index rebuild/repair tests pass with the LanceDB build tag.
- Final end-to-end run from the changed source (848 parsed files): parse 1.69s, cache save 0.02s, graph prepare 0.61s, graph export 0.32s, graph publish 0.00s, embeddings 0.20s, search open 0.00s, search build 26.62s, search maintenance 0.57s, search close 0.00s; broad write time 28.33s and total 30.0s. Progress visibly changed from cache save to graph write, search build, and search maintenance at the correct boundaries.
- Final validation passed: `go test -tags lancedb -count=1 -timeout 600s ./internal/ast ./cmd/graphit/commands`, `go vet -tags lancedb ./internal/ast ./cmd/graphit/commands`, and `git diff --check`.
- Recorded the unrelated 848-files/854-shards discrepancy in the improvement backlog for a separate correctness investigation.
- Persisted the measured root cause and new instrumentation contract in project memory, and updated the stale Shutdown/Close memory to mark that explanation as superseded.
- User requested a direct commit on `main`; confirmed the active branch is `main`, `git diff --check` passes, and the seven modified/untracked paths are all artifacts of this task before staging them together.
