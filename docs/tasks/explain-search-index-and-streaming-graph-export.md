---
title: Explain and reduce search-index and graph-write latency
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [ast, lance, ivf-pq, performance]
---

# Explain search-index cost and streaming graph export

## Objective

Explain precisely what the post-graph `Building search index` phase does, why its IVF-PQ step dominates wall time, and whether Icebug/Parquet graph preparation or writing can overlap file parsing instead of rereading completed parse shards after the parse barrier.

## Reasoning and Approach

The answer must distinguish work that is inherently corpus-global from work that can be accumulated per parsed file. It must also preserve current correctness constraints: graph publication is atomic, relationships may target declarations parsed later, full rebuild output must exclude vanished shards, and `ShardCache.StreamEntries` has destructive behavior on an unflushed cache. Project memory, the knowledge wiki, the AST graph, and current source entities are the sources of truth. The Hub has no LanceDB/vector-database artifact, so no external API behavior will be assumed.

## Plan & Task Breakdown

- [x] **T1 — Decompose search-index construction** — Spec: trace row generation, table writes, FTS/scalar/vector index creation, and maintenance; identify which steps require the complete corpus.
- [x] **T2 — Trace the parse-to-shard-to-Parquet lifecycle** — Spec: locate the parse worker output boundary, shard save/adoption semantics, graph rebuild input, and graph publication barrier.
- [x] **T3 — Assess overlap designs** — Spec: classify node and relationship work into safe per-file preparation, concurrent append, deferred global resolution, and atomic publication; account for memory, I/O contention, determinism, deletion, and failure recovery.
- [x] **T4 — Report recommendation** — Spec: answer what is possible now, what is not safely streamable, and the smallest measured experiment that should precede implementation.
- [x] **T5 — Establish baselines and impact surface** — Spec: query callers/tests for the parse-cache, rebuild-index, graph-export, and Lance index entities; capture bundle identity, phase timings, and peak-memory baselines before changing behavior.
- [x] **T6 — Pipeline graph preparation with parsing** — Spec: introduce an `AdoptEntry`/`Finalize` boundary or equivalent single-pass preparation that consumes each fresh entry before shard eviction, preserves unchanged cached entries and all global resolution invariants, and removes redundant post-parse decode work without removing durable shards.
- [x] **T7 — Defer only IVF-PQ to `sync --heavy`** — Spec: keep LanceDB table replacement, source/text rows, FTS, B-tree, and bitmap inside `ast index`; mark the vector index pending for the current corpus generation; build/finalize IVF-PQ only in the heavy sync path and discard work whose generation became stale.
- [x] **T8 — Verify equivalence and performance** — Spec: compare old/new bundle contents and queries, exercise full/force/incremental/deletion/search-repair paths, run affected suites and vet, and record measured phase and memory changes.

## Use Cases

### UC-01: Understand and reduce post-parse indexing latency

- **Actor**: Graphit engineer indexing a large repository.
- **Preconditions**: Parsing emits durable shards; the current direct Icebug/Parquet and LanceDB paths are enabled.
- **Main Flow**:
  1. Identify every operation behind `Building search index`.
  2. Separate corpus-local from corpus-global work.
  3. Identify work that can overlap parsing without publishing partial state.
  4. Recommend a staged optimization with measurable acceptance criteria.
- **Alternative Flows**: When a freshly parsed file replaces a cached shard, the preparation stage adopts the fresh entry; when parsing fails, it retains the last durable cached entry.
- **Error Scenarios**: Preparation cancellation or shard-read failure aborts graph publication; no partial Parquet bundle is renamed into place.
- **Postconditions**: The engineer can decide whether to pipeline preparation, graph writing, search-row ingestion, or index creation without weakening correctness.
- **Affected Files**: `internal/ast/rebuild_prepare.go`, `internal/ast/pipeline.go`, `internal/ast/icebug_rebuild.go`, `internal/ast/shard_cache.go`.

### UC-02: Publish a functional search index without synchronous IVF-PQ

- **Actor**: `ast index`, daemon AST reindex, or phase 1 of `sync`.
- **Preconditions**: Parse shards are durable and the local Lance store is writable.
- **Main Flow**:
  1. Publish a new corpus generation as `vector_index: pending`.
  2. Replace or incrementally update `files` and `entities`, including source text and cached vectors.
  3. Build/fold FTS, B-tree, and bitmap indexes.
  4. Leave semantic search operational through exact vector scan.
- **Alternative Flows**: Incremental updates remove the previous generation's IVF-PQ before changing rows.
- **Error Scenarios**: A failed status publication aborts before table mutation; a failed table/index write leaves the generation pending and never claims readiness.
- **Postconditions**: Text search and `ast source` match the new AST generation; IVF-PQ is not on the synchronous critical path.
- **Affected Files**: `internal/ast/search_lance.go`, `internal/lancestore/store_lancedb.go`, `internal/ast/pipeline.go`.

### UC-03: Finalize IVF-PQ for the current generation

- **Actor**: `sync --heavy`, `ast embed`, or the background embedding cycle.
- **Preconditions**: The functional Lance tables exist and `embeds.json` identifies a pending generation.
- **Main Flow**:
  1. Capture the current corpus generation before scanning/generating embeddings.
  2. Generate only missing embeddings, or generate none when the binary cache is complete.
  3. Serialize IVF-PQ finalizers and train IVF-PQ when at least 256 vectors exist.
  4. Recheck the generation under the status lock and publish `ready` only when it still matches.
- **Alternative Flows**: Below 256 vectors, exact scan is published as the ready policy without creating IVF-PQ; an already-ready generation returns without rebuilding.
- **Error Scenarios**: A stale generation is discarded and cannot overwrite newer status; a concurrent finalizer skips duplicate work.
- **Postconditions**: The current generation is `ready`, or remains `pending` if a newer AST generation superseded the work.
- **Affected Files**: `internal/ast/embedder.go`, `internal/ast/search_lance.go`, `cmd/graphit/commands/ast.go`, `cmd/graphit/commands/lifecycle.go`.

## Test Cases & Acceptance Criteria

### Scenario: Proposed overlap preserves the complete graph

Ref: UC-01

```gherkin
Given files may reference declarations parsed later
When graph work overlaps parsing
Then no relationship is dropped because its target was unavailable at the time its source file completed
  And the published bundle appears atomically only after all required global work succeeds
```

### Scenario: Proposed overlap reduces the post-parse critical path

Ref: UC-01

```gherkin
Given phase timings for parse, shard decode, graph export, and search build
When an overlap experiment is implemented
Then work moved into the parse window is reported separately
  And end-to-end wall time decreases without merely shifting CPU or I/O contention
```

### Scenario: AST publication defers only IVF-PQ

Ref: UC-02

```gherkin
Given a corpus whose cached entity vectors are present
When ast index rebuilds the graph and search tables
Then files, entities, source text, FTS, B-tree, and bitmap are published
  And vector_index is pending for a new generation
  And semantic search can answer through exact scan
  And IVF-PQ is not trained in ast index
```

### Scenario: Heavy publishes only the generation it trained

Ref: UC-03

```gherkin
Given sync --heavy captured corpus generation X
  And ast index publishes corpus generation Y before IVF-PQ finalization completes
When the heavy finalizer attempts to publish its result
Then generation Y remains pending
  And generation X is not marked ready
  And the stale IVF-PQ result is discarded
```

### Scenario: Heavy finalizes with zero new embeddings

Ref: UC-03

```gherkin
Given every current entity embedding exists in the binary shard cache
  And vector_index is pending
When sync --heavy runs
Then zero embeddings are generated
  And IVF-PQ is finalized for the current generation
  And vector_index becomes ready
```

## Progress Log

### 2026-08-31

- Searched project memory and knowledge for the current Icebug, shard-cache, and search-index architecture.
- Confirmed the Hub has no LanceDB or generic vector-database knowledge artifact; analysis will use project-owned code and documentation.
- Traced `SearchIndex.RebuildFromCache`: a full rebuild drops/recreates `files` and `entities`, streams shard rows in 2,000-row batches, then creates files FTS/path B-tree and entities FTS/IVF-PQ/etype bitmap/path B-tree. Embeddings are replayed from the embedding cache; the model is not run in this phase. Measured IVF-PQ creation remains the dominant operation (22.914s of a 25.667s search rebuild).
- Traced the live parse boundary: each completed `ParsedFile` is converted and stored immediately, and dirty shards are flushed/evicted every 100 parsed files. After the parse barrier, the graph rebuild streams those durable shards back, retains every decoded entry in a corpus-sized map, constructs `rebuildIndex`, and scans it globally before any Parquet write.
- Confirmed the global barriers: node primary-key sort defines dense per-table IDs; relationship CSR files require those IDs; calls/DML/field references require the complete declaration index; labels, relation endpoint pairs, and schema arise from the complete corpus; output is published atomically only after a finished manifest.
- Concluded that final canonical Parquets cannot safely be appended per parsed file under the current invariants, but preparation can overlap parsing. The recommended shape is an online rebuild accumulator fed directly from each completed parse plus compact per-label/per-relation spool files. The terminal phase then resolves global references, sorts primary keys, assigns IDs, builds CSR, emits one-row-group Parquets, writes the manifest, and renames atomically. This removes the JSON shard reread and the `entries`/`fileEntries` retention without publishing partial data.
- Search rows can likewise be appended to a temporary LanceDB sidecar while parsing, but FTS and especially IVF-PQ still belong after bulk ingestion; building indexes per batch would pay index maintenance repeatedly. The row append measured only 1.735s, so overlapping it helps much less than changing/reusing the 22.914s IVF-PQ build.
- The user authorized implementation of the recommendation. Re-read the existing task log and project memories before editing; the implementation phase will preserve the durable shards and the current full/incremental fallback semantics rather than treating them as removable intermediates.
- Direction correction from the Engineer: do not reuse/fold IVF-PQ in the normal rebuild and do not defer the whole search sidecar. The normal AST path must remain fully functional for text search and `ast source`; only the measured 22.914s IVF-PQ training moves to `sync --heavy`. The in-progress fingerprint/reuse prototype is being removed before further implementation.
- Revalidated the corrected design before implementation. The AST rebuild will publish a new corpus generation as `vector_index: pending` before replacing/mutating Lance tables, recreate `files`/`entities`, restore cached vectors, and build only FTS/B-tree/bitmap indexes. Incremental updates will remove any IVF-PQ index before mutating rows so `pending` cannot silently expose an index trained for the previous generation.
- Found a heavy-path correctness gap: `Embedder.RunCycle` returns immediately when the durable embedding cache has no pending vectors. That is the common post-rebuild case, so `sync --heavy` currently has no opportunity to build IVF-PQ. The implementation must finalize a pending generation even when zero new embeddings are generated.
- Chosen publication protocol: heavy captures the generation before embedding/finalization, serializes IVF-PQ builders, and changes `pending` to `ready` only if that generation is still current. A stale result is not published and its stale IVF-PQ index is removed. Status writes are atomic and cross-process serialized so an older heavy run cannot overwrite a newer AST generation.
- Added `lancestore.Table.DropIndex` with matching Lance-enabled/disabled surfaces. It resolves the deterministic wrapper index name, treats absence as success, and is the primitive the AST incremental path will use to remove an IVF-PQ trained for the previous generation.
- Extended `embeds.json` with a corpus generation and `vector_index` publication state. Generation changes and status transitions now use a cross-process lock plus temp-file rename, preventing torn JSON and preventing a completed heavy run for generation X from overwriting generation Y.
- Removed IVF-PQ from `SearchIndex.RebuildFromCache`; the synchronous rebuild now creates only entity FTS, `etype` bitmap, and `path` B-tree after loading all rows. Full and incremental writes publish a fresh pending generation before table mutation. Incremental writes also remove the old IVF-PQ first and now restore cached embeddings through the previously ignored `embLookup` argument.
- Split vector finalization into a generation-aware operation. `FinalizeVectorsForGeneration` serializes concurrent IVF-PQ builders, checks the captured generation before work and again at publication, publishes `ready` only for the current corpus, and removes a stale IVF-PQ result instead of exposing it. Below Lance's 256-vector training floor it publishes `ready` with exact scan semantics and no IVF-PQ.
- Changed `Embedder.RunCycle` to capture the corpus generation before scanning/writing vectors and finalize that exact generation. A cycle with zero pending embeddings now still runs vector finalization, closing the gap where restored cached vectors left IVF-PQ pending forever; finalization errors now reach the heavy caller.
- Updated explicit `ast embed` so its zero-pending fast path finalizes the vector index after any search-table repair instead of returning before IVF-PQ publication.
- Reworked `sync --heavy` embedding setup so it opens the search index and caches before constructing the embedding provider. When no embeddings are pending, heavy can now build/publish IVF-PQ without loading a model or requiring provider initialization; when embeddings are pending, the provider is created as before and the same cycle finalizes the captured generation.
- Added Lance integration regressions for the new contract: rebuild publishes pending while exact vector scan remains usable; heavy finalization publishes ready; a stale generation cannot publish; and incremental replacement restores its cached vector while advancing back to pending.
- Added a zero-pending embedding-cycle regression using a real parse cache, binary embedding cache, and local Lance store. It proves cached embeddings cause no model work while the cycle still moves the current vector generation from pending to ready.
- Removed the obsolete vector-index filtering helper and updated stale comments so the source states the new ownership boundary: synchronous AST builds functional indexes; the heavy finalizer owns IVF-PQ.
- Hardened repeated/background cycles: a generation already marked ready returns before counting, folding, or maintenance, so the daemon's two-minute embedding loop does not repeatedly optimize a finished index. Legacy stores with no generation are upgraded through the normal pending-generation path, and failure to physically drop an already-obsolete IVF-PQ no longer lets an old heavy run overwrite current status.
- End-to-end reindex measurement on 850 parsed files / 863 durable shards: total 6.3-6.5s, parse 1.65-1.76s, write 4.62-4.68s, graph preparation 0.05s, graph export 0.33-0.35s, embedding-cache lookup 0.14-0.17s, search rebuild 3.52-3.53s, and search maintenance 0.57-0.58s. The pre-change search rebuild was 25.667s, including 22.914s of IVF-PQ.
- Measured the isolated heavy finalization after a second rebuild with all vectors cached: zero embeddings generated, 27.8s for IVF-PQ/fold/maintenance. Re-running heavy on the already-ready generation took 1.6s and did not rebuild IVF-PQ. A prior heavy run that also generated 1,518 embeddings took 178.3s and is recorded separately rather than conflated with IVF-PQ cost.
- Revalidated the Engineer's publication direction before final commit: `pending` is intentionally operational through exact vector scan, while source text, FTS, B-tree, and bitmap remain synchronous. The CLI timing output will label graph preload as overlapped so operators do not add it to the write critical path.
- Final verification passed with `go test -count=1 -tags lancedb ./internal/lancestore ./internal/ast ./cmd/graphit/commands` and `go vet -tags lancedb ./internal/lancestore ./internal/ast ./cmd/graphit/commands`. Earlier targeted race coverage also passed for rebuild preparation and vector-generation publication.
- Repository-wide verification also passed with `go test -count=1 -tags lancedb ./...`.

## Recommendation

1. Treat the graph change as a single-pass rebuild-index redesign, not direct per-file writes into final Parquets. Reuse the existing backlog item about corpus-sized Icebug rebuild residency instead of creating a duplicate.
2. Prototype an `AdoptEntry`/finalize split: feed entries before shard eviction, spool compact unresolved facts, and compare end-to-end time, peak heap, JSON bytes reread, and bundle identity against the current path.
3. Keep IVF-PQ generation-owned and heavy-only. Do not reuse it across AST generations in the synchronous path; exact scan is the pending behavior and the heavy finalizer is the only transition to ready.

## Trade-offs & Decisions

- The accepted architecture is recorded in `docs/decisions/defer-ivf-pq-to-heavy-sync.md`.
- Only IVF-PQ moved to heavy. Deferring the entire Lance rebuild was rejected because it would make text search and source retrieval stale or empty.
- A random generation token is used instead of a content hash. It also invalidates IVF-PQ when parser/index behavior changes while file bytes remain identical.
- Incremental AST updates drop the previous IVF-PQ rather than attempting in-place reuse. This preserves the meaning of pending and prevents an index trained for old rows from being selected implicitly by Lance.
- Below the 256-vector training floor, ready means the vector channel is finalized with exact scan; it does not falsely require an IVF-PQ that Lance cannot train.

## Technical Debt

- [ ] The graph preparation overlap currently retains prepared entries in a corpus-sized map. It removes redundant post-parse shard decoding but does not yet implement the compact spool design needed to reduce peak heap to bounded working state.
- [ ] `sync --heavy` reports one combined task duration for missing-embedding generation plus IVF-PQ/fold/maintenance. Follow-up is recorded in [Split sync --heavy timing into embedding generation and IVF-PQ finalization](backlog/split-sync-heavy-timing-into-embedding-generation-and-ivf-pq.md).

## System Knowledge

- `Embedder.RunCycle` previously returned before `FinalizeVectors` when zero embeddings were pending; restored cached vectors therefore could leave IVF-PQ absent indefinitely.
- LanceDB exposes `DropIndex` in the pinned Go binding. The project wrapper now uses deterministic index names to provide idempotent removal.
- A ready-generation fast path is necessary because the daemon invokes the embedding cycle every two minutes; without it, zero-pending cycles would repeatedly count/fold/maintain the finished index.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/explain-search-index-and-streaming-graph-export.md` | Created | Record the current phase decomposition, global correctness barriers, and safe overlap design. |
| `internal/lancestore/store_lancedb.go` | Modified | Expose idempotent removal of one named Lance index for generation invalidation. |
| `internal/lancestore/store_disabled.go` | Modified | Keep the non-Lance build API identical. |
| `internal/ast/search_lance.go` | Modified | Add atomic, generation-aware vector-index status publication. |
| `internal/ast/embedder.go` | Modified | Finalize the captured corpus generation even when no embeddings are newly generated. |
| `cmd/graphit/commands/ast.go` | Modified | Make the explicit embed command finalize a pending vector index on its zero-work path. |
| `cmd/graphit/commands/lifecycle.go` | Modified | Let `sync --heavy` finalize IVF-PQ independently of embedding-model work. |
| `internal/ast/search_lance_test.go` | Modified | Cover pending/ready publication, stale-generation rejection, scan fallback, and incremental vector restoration. |
| `docs/decisions/defer-ivf-pq-to-heavy-sync.md` | Created | Record why only IVF-PQ is heavy and define the generation-safe pending/ready contract. |
| `internal/ast/rebuild_prepare.go` | Created | Preload unchanged shard entries during parsing and adopt fresh parsed entries before eviction. |
| `internal/ast/rebuild_prepare_test.go` | Created | Cover unchanged/fresh composition and failed-reparse fallback. |
| `internal/ast/shard_cache.go` | Modified | Stream all entries except paths already supplied by preparation. |
| `internal/ast/pipeline.go` | Modified | Start preparation beside parsing and pass the completed entry set into graph rebuild. |
| `internal/ast/icebug_rebuild.go` | Modified | Rebuild from prepared entries without rereading the complete shard set after parsing. |
| `cmd/graphit/commands/runners.go` | Modified | Report graph preload separately and mark it as overlapped rather than additive write time. |

---
