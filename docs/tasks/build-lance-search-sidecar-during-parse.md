---
title: Build Lance search sidecar during parse
status: complete
created: 2026-08-31
updated: 2026-08-31
tags: [ast, lance, search, performance, publication]
---

# Build Lance Search Sidecar During Parse

## Objective

Overlap the full-rebuild Lance row preparation and table ingestion with AST parsing instead of rereading every completed parse shard after graph publication. The Linux benchmark leaves 122.92 seconds in row preparation and 43.65 seconds in Lance writes after a 151.92-second parse, so a generation-scoped staging sidecar can remove most of that serialized tail. The same change also removes the full source-text duplication between `files.source` and `files.body`: one reversible search document becomes both the FTS input and the queryable source owner.

The active search sidecar must never expose partial rows. Parsing feeds a bounded staging builder only after a file result is accepted into the durable cache; graph success remains the gate for publishing the completed search store. Parse, staging, graph, or index failures must leave the previously published search sidecar queryable. The accepted IVF-PQ decision remains unchanged: synchronous publication includes source, text indexes, scalar indexes, and cached vectors with `vector_index: pending`; heavy vector finalization owns IVF-PQ and `ready`.

## Plan & Task Breakdown

- [x] **T1 — Map the full-rebuild lifecycle and invariants** — Spec: trace `runFileWorkerPool`, parse-result adoption, cache persistence, `RebuildFromCache`, vector-generation publication, and existing directory swaps; identify tested callers and failure paths before editing.
- [x] **T2 — Add a bounded staging search builder** — Spec: accept durable parse entries and source as they arrive, prepare and append file/entity rows in bounded batches, retain cached embeddings, propagate cancellation/errors, and never mutate the active `search.lance` directory.
- [x] **T3 — Make `files.source` the sole file-text owner** — Spec: index a reversible document containing filename/path search terms plus the exact source in the `source` column, remove `body` completely, preserve exact `ast source`, file search, current-schema incrementals, and remote publication. Old Lance schemas require a rebuild; this development branch carries no compatibility shim.
- [x] **T4 — Publish staging atomically after graph success** — Spec: finish FTS/scalar indexes in staging, close all Lance handles, replace only the active search sidecar with rollback/cleanup behavior, and publish vector generation/counts for the same completed corpus.
- [x] **T5 — Integrate the forced full-rebuild path** — Spec: enable overlap where the full strategy is known before parsing, retain the current incremental path and a safe fallback for late full-rebuild decisions, and report preparation/write/index/publication timing without double-counting.
- [x] **T6 — Cover correctness and failure behavior** — Spec: test single-copy schema, byte-exact source recovery, filename/path/content recall, source/search equivalence, hidden staging before publish, preservation of the active sidecar on failure, bounded pipeline completion, cached-vector restoration, and pending/ready generation semantics.
- [x] **T7 — Verify and benchmark** — Spec: run focused and repository-wide Lance-tagged tests, vet the changed packages, benchmark the Linux reset corpus if practical, and compare serialized tail, total wall time, and `files.lance` bytes to the 423.0-second / 3.16-GB baseline.
- [x] **T8 — Document, reflect, and commit main** — Spec: keep this log and affected architecture/decision docs current, remove both completed backlog items, record durable memory, synchronize all indexes, and commit the verified change directly on `main` as requested for this workstream.

## Use Cases

### UC-01: Full AST reset overlaps search construction with parsing

- **Actor**: User running `graphit ast index --reset` or another forced full rebuild.
- **Preconditions**: The full-rebuild strategy is known before parse workers start and the target store can host a sibling staging directory.
- **Main Flow**:
  1. The pipeline creates a private generation-scoped Lance staging store.
  2. Each accepted parse result is persisted to the shard cache and submitted through a bounded channel.
  3. The staging builder writes file/entity rows while other files continue parsing.
  4. After parse and graph success, the builder creates FTS/scalar indexes and closes staging.
  5. The pipeline atomically replaces the active search sidecar and publishes the pending vector generation/counts.
- **Alternative Flows**: Incremental indexes and full rebuilds chosen only after parsing retain the existing post-parse search path until an equivalent staging source is available.
- **Error Scenarios**: Any parse, builder, graph, index, close, or swap error cancels/drains the builder, removes staging, and does not replace a previously active sidecar.
- **Postconditions**: Graph, source retrieval, textual search, and cached-vector exact scan describe the same completed corpus.
- **Affected Files**: To be populated after structural mapping.

### UC-02: Heavy vector finalization follows staged publication

- **Actor**: Background embedding cycle, `ast embed`, or `sync --heavy`.
- **Preconditions**: A staged synchronous rebuild has published a new corpus generation as pending.
- **Main Flow**: Existing heavy finalization captures that generation, fills missing vectors, trains IVF-PQ when eligible, and marks only the still-current generation ready.
- **Alternative Flows**: Corpora below the training floor finalize with exact scan.
- **Error Scenarios**: A stale heavy result is discarded and cannot mark a newer staged corpus ready.
- **Postconditions**: Staging does not weaken the accepted generation protocol.
- **Affected Files**: To be populated after structural mapping.

### UC-03: One stored file document serves FTS and exact source retrieval

- **Actor**: User invoking `ast search`, `ast source`, bundle export, or a remote mounted AST context.
- **Preconditions**: The file row was written by a full or incremental Lance path.
- **Main Flow**: The row stores one reversible document in `files.source`; file FTS indexes that column; source readers remove only the deterministic metadata suffix and return the original bytes.
- **Alternative Flows**: None for old schemas. This project is in development and a schema change requires a full rebuild.
- **Error Scenarios**: Empty or malformed rows do not fabricate source and preserve the existing not-found behavior.
- **Postconditions**: New full rebuilds contain no `files.body` column and retain filename/path/content recall plus byte-exact source retrieval.
- **Affected Files**: To be populated after implementation.

## Test Cases & Acceptance Criteria

### Feature: Concurrent staged search publication
Ref: UC-01

#### Scenario: Full reset publishes a complete staged sidecar

```gherkin
Given an active graph and search sidecar and a forced full rebuild with multiple source files
When parse results are cached and streamed to the bounded staging builder
Then the active search sidecar remains unchanged until graph and staging indexes succeed
  And the published sidecar returns every new source and textual search result
```

#### Scenario: Failed staged build preserves the prior search corpus

```gherkin
Given a queryable active search sidecar
When the generation-scoped staging builder fails before publication
Then the full index operation returns the failure
  And the prior active sidecar remains queryable
  And no staging directory is treated as active
```

#### Scenario: Pipeline bounds in-flight search work

```gherkin
Given more parsed files than the staging channel capacity
When Lance ingestion is slower than parsing
Then producers apply backpressure instead of retaining the whole corpus in memory
  And cancellation releases every producer and consumer without leaking a goroutine
```

### Feature: Vector generation remains coherent
Ref: UC-02

#### Scenario: Staged publication is pending until heavy finalization

```gherkin
Given a staged full rebuild containing cached and missing entity vectors
When the sidecar is published
Then its generation is pending with accurate total and pending counts
  And exact vector scan remains available for cached vectors
When heavy finalization completes for the same generation
Then that generation becomes ready
```

#### Scenario: Stale heavy work cannot publish ready

```gherkin
Given heavy finalization captured generation A
When staged publication replaces it with generation B before finalization completes
Then generation A cannot mark generation B ready
```

### Feature: Single-copy file text
Ref: UC-03

#### Scenario: File source is stored once and recovered exactly

```gherkin
Given a file path `internal/hub/registry.go` and source containing newlines and control-like text
When its Lance file row is rebuilt
Then the file schema has `path`, `name`, and `source` but no `body`
  And `ast source` returns the exact original source
```

#### Scenario: One source document preserves file recall

```gherkin
Given a file whose path is `internal/memory/scope.go` and whose body contains `OpenScope`
When a user searches by filename, path token, or source token
Then the file remains discoverable through FTS on `files.source`
```

## Implementation Details

- `files.source` now stores `source + NUL + filename/path metadata`. The final NUL is unambiguous because filesystem paths cannot contain it; source readers split at the last boundary, so source text containing NUL still round-trips.
- File FTS and file queries target `source`. The schema and every writer omit `body`; old indexes must be rebuilt.
- `FileSource`, `EachFileSource`, and file search results decode the document before exposing source.
- Forced full rebuilds create `search.lance.staging.<generation>` before parse workers start. A 16-entry bounded queue receives only cache-accepted parse entries; the consumer reuses the full-rebuild writer and closes the Lance/embedding-cache handles before publication.
- The consumer completes FTS and scalar indexes after the queue closes, concurrently with graph export. The active search directory is untouched until both the graph export and staged search build succeed.
- Graph publication now retains the previous bundle as a sibling backup. Search publication acquires the vector-finalization lock and swaps an already-closed staging directory; a failed search publication rolls the graph back before returning. Commit and rollback use destination-free sibling renames, which are portable across Linux, macOS, and Windows.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/build-lance-search-sidecar-during-parse.md` | Created | Open the implementation with objective, invariants, tests, and resumable tasks. |
| `internal/ast/search_lance.go` | Modified | Make `files.source` the sole stored file document and preserve exact source decoding. |
| `internal/ast/search_lance_test.go` | Modified | Assert the no-`body` schema, exact source round trip, and filename/path/content recall against real Lance. |
| `internal/ast/search_staging.go` | Added | Build, close, publish, and roll back a generation-scoped Lance sidecar. |
| `internal/ast/search_staging_test.go` | Added | Verify staging invisibility, abort preservation, and failed-swap restoration against real Lance. |
| `internal/ast/pipeline.go` | Modified | Stream forced full rebuild parse results into staging and overlap index construction with graph work. |
| `internal/ast/icebug_rebuild.go` | Modified | Retain the prior graph bundle until paired search publication commits. |
| `internal/ast/icebug_publication_test.go` | Added | Verify portable graph publication commit and rollback directory semantics. |

## Trade-offs & Decisions

- Start with the forced full-rebuild path because its strategy is known before parsing and it covers the measured Linux `--reset` benchmark. Do not speculate a duplicate staging path for incrementals before the full path is correct.
- Keep FTS/scalar creation after all staged rows arrive; Lance index builders require complete tables, while row preparation and appends are the overlap target.
- Keep IVF-PQ out of this change. The accepted ADR assigns it to generation-safe heavy finalization.

## Technical Debt

- [ ] Decide from measurements whether late-selected full rebuilds should spool accepted entries during every incremental attempt or retain the post-parse fallback.
- [ ] Tune staging concurrency/backpressure independently: the 16-entry queue keeps memory bounded but raises Linux parse wall time from 151.92s to 240.96s under concurrent Lance preparation. The total still improves, but a larger bounded queue or separately budgeted preparation workers may recover parser throughput.
- [x] The duplicate source/body representation was explicitly folded into this task at the user's request; it must remain a separate implementation seam and measurement inside the same commit.

## System Knowledge

- The Linux benchmark establishes a 423.0-second baseline: parse 151.92s, graph preparation/export 42.51s, search preparation/writes 166.57s, and search index creation 57.97s.
- Search tables and source text are synchronous publication state; only IVF-PQ is deferrable to heavy work.

## Progress Log

### 2026-08-31

- Reopened the memory, knowledge, AST, and improvement rules for this new task.
- Read the staged-sidecar backlog, the accepted IVF-PQ ADR, the Linux benchmark log, and the important benchmark memory.
- Confirmed the intended boundary: private generation-scoped staging receives durable parse results; graph success gates atomic search publication; active tables are never mutated during parse.
- The user expanded the task to remove the duplicate source text between `files.source` and `files.body`.
- Re-ran memory, wiki, and AST lookups after the scope change. The standing source-ownership decision remains: shards never carry source and Lance remains the local/remote queryable owner.
- Chose a reversible single-column document: FTS indexes `files.source`, while source readers strip a deterministic metadata suffix. This preserves filename/path recall without duplicating the full source.
- Implemented the single-owner schema as a source suffix envelope (`source + NUL + metadata`) and decoded it at all public source exits.
- Focused real-Lance tests passed for content/name/path FTS, exact source retrieval, existing source-service ownership, and the standard rebuild query path.
- The user clarified that no backward compatibility is required for these development-time storage changes. Removed the legacy `body` shim: old Lance schemas must be rebuilt.
- The user also made cross-platform operation explicit. Publication must close Lance handles before directory moves and use explicit staging/backup/rollback sequencing that works on Linux, macOS, and Windows.
- Implemented the bounded staged builder and integrated it only for forced full Ladybug rebuilds; other backends and incremental/late-selected full rebuilds retain the existing path.
- Added paired publication compensation: the previous graph bundle stays recoverable until the staged Lance sidecar and vector-generation counts publish successfully.
- Focused real-Lance staging/source tests and portable graph commit/rollback tests pass.
- First Linux reset benchmark exposed a command-to-pipeline contract gap: `--reset` deleted the store but passed `ForceRebuild=false`, so the staging guard never activated. Result: 164.44s parse, 315.24s write, 480.2s total, with the old serialized 162.50s preparation still present. Corrected `runASTIndex` to mark both `--reset` and `--reindex` as forced full rebuilds.
- Final Linux reset with staging active: 240.96s parse, 86.01s write, and 327.5s total. Compared with the 423.0s baseline, total wall time fell 95.5s (22.6%) and the post-parse write tail fell 184.57s (68.2%). `search-build` after graph export fell from 224.55s to 13.63s because 186.82s preparation and 50.36s table writes occurred during parse; FTS/scalar work overlapped graph export.
- Clarified the CLI timing vocabulary for staged runs: the 13.63s residual is now printed as `search-wait (build overlapped)`, while the detailed search phases are labelled active overlapped work. Non-staged runs retain the existing `search-build` label.
- The staging contention is visible rather than hidden: parse rose from 151.92s to 240.96s. This is a bounded-memory trade-off, not a free overlap, and is recorded as the next tuning seam.
- Single-copy storage reduced the complete Linux AST store from 7,843,802,219 to 6,304,454,225 apparent bytes (19.6%). `search.lance` fell from 7,265,877,230 to 5,726,549,880 bytes (21.2%), and `files.lance` fell from approximately 3.16GB to 1,634,623,636 bytes (about 48%). Shards remained 326,928,682 bytes and Icebug 239,627,024 bytes.
- Verified the published Linux index through hybrid search (`schedule` returned `__schedule` and `schedule` from `kernel/sched/core.c`) and byte-visible `ast source` output for that file.
- Verification passed: repository-wide `go test -count=1 -tags lancedb ./...`, focused `go vet -tags lancedb`, and race tests for staging/publication. Local cross-compilation remains blocked by the repository's existing native tree-sitter/CGO toolchains; release builds provide the actual macOS/Windows compilation environments. The new publication path itself uses destination-free sibling renames after closing Lance handles, with portable commit/rollback tests.
- Post-task reflection removed the two completed backlog briefs and opened `tune-staged-lance-backpressure-to-recover-parser-throughput` for the measured parser contention plus age-guarded cleanup of crash-left staging/backup directories. No unrelated cleanup was folded into this commit.
- Updated the durable Linux benchmark memory in place with the staged-publication design, single-copy schema, final timings/storage, portability constraint, and remaining measured contention.
- Next: extract the full-rebuild row writer and drive it through a bounded staging consumer during forced parsing.
