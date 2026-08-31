---
status: completed
---

# Profile Linux corpus index time and storage

## Objective

Use the registered Linux repository as the large-corpus benchmark to explain and reduce the 712.31-second AST write phase and the approximately 4 GB local AST store produced from roughly 1 GB of source files. Changes to storage formats, publication methods, and indexing algorithms are in scope when measurements justify them.

## Reasoning and Approach

The benchmark must separate graph preparation/export from Lance row ingestion, index creation, and maintenance, then attribute disk usage by durable artifact and by duplicated logical payload. The investigation will not assume UID repetition is the cause: it will measure cardinality, serialization overhead, fragment/version retention, and source/vector duplication. The Linux project is a registered sibling, so its graph and metadata are accessed through Graphit tools; its global store is inspected only for runtime facts that the AST graph cannot contain.

## Plan & Task Breakdown

- [x] **T1 — Establish benchmark identity and inventory** — Resolve the Linux project/store identity, record exact artifact sizes, file counts, Lance fragment/version/index sizes, and graph table sizes.
- [x] **T2 — Decompose the 606.82-second search build** — Measure row production/write, FTS, scalar indexes, vector handling, and maintenance independently on the Linux corpus.
- [x] **T3 — Quantify serialization and duplication** — Measure shard JSON keys and payloads, UID/path/name cardinality, embedding-cache duplication, source-text duplication, and compression ratios.
- [x] **T4 — Rank design alternatives** — Compare binary/columnar shard formats, derived or dictionary-encoded identifiers, source/vector ownership, Lance ingestion strategies, and maintenance policy by expected disk/time benefit and migration risk.
- [x] **T5 — Implement measured high-confidence wins** — Apply the smallest changes that materially reduce wall time or disk without weakening graph/search publication correctness.
- [x] **T6 — Verify on focused tests and the Linux benchmark** — Run correctness/race/vet suites, rebuild or probe the Linux store proportionally to cost, and record before/after measurements.

## Use Cases

### UC-01: Diagnose a large AST write

- **Actor**: Graphit engineer indexing a large repository.
- **Preconditions**: A completed `ast index --reset` exposes phase timings and a durable local store.
- **Main Flow**: Attribute time and bytes to specific subphases and artifacts, then identify the dominant scalable terms.
- **Error Scenarios**: A probe must not mutate or rebuild the production store unless explicitly part of verification.
- **Postconditions**: Every recommendation is tied to a measured time/size contribution.

### UC-02: Reduce local AST storage

- **Actor**: Graphit engineer maintaining large project indexes.
- **Preconditions**: Graph, search, source retrieval, and rebuild caches have explicit ownership contracts.
- **Main Flow**: Remove duplicated representations or replace inflated serialization while preserving restart/rebuild behavior.
- **Postconditions**: Disk usage falls without making `ast search`, `ast source`, incremental rebuilds, or heavy vector finalization inconsistent.

## Test Cases & Acceptance Criteria

### Scenario: Disk recommendation is evidence-based

```gherkin
Given the Linux AST store is approximately four times the source tree size
When its durable artifacts and repeated fields are measured
Then the report identifies the exact largest byte contributors
  And distinguishes repeated logical values from serialization and index overhead
```

### Scenario: Performance change preserves publication correctness

```gherkin
Given the Linux corpus exercises full graph and search publication
When a storage or ingestion optimization is applied
Then graph bundle contents and search/source results remain equivalent
  And pending/ready vector generation semantics remain intact
```

## Progress Log

### 2026-08-31

- Recorded the Engineer's Linux benchmark: 73,635 files, 160.42s parse, 712.31s write, 873.2s total; graph prepare 7.02s, graph export 35.49s, search build 606.82s, and search maintenance 62.36s.
- Resolved `~/projects/linux` as a registered sibling project. It currently has no project knowledge pages or memories.
- Recalled prior measured storage behavior: on the Graphit repository, binary vectors stored as decimal JSON in shard embedding sidecars consumed 55% of the store and duplicated vectors already stored in Lance. This is a hypothesis to verify on Linux, not a conclusion about its store.
- Measured the Linux source tree at 1,640,314,422 bytes and its complete AST store at 20,583,557,177 apparent bytes (20,954,652,672 allocated bytes). The earlier approximately 4 GB observation describes only the parse shards, not the complete store.
- Attributed the store: `search.lance` is 15,482,052,317 bytes (75.2%), `shards` is 4,851,704,355 bytes (23.6%), `graph.icebug` is 238,451,866 bytes (1.2%), and `manifest.json` is 11,348,517 bytes. The Parquet graph is therefore not the storage bottleneck.
- Split Lance storage: `entities.lance` is 9,168,289,987 bytes, including 6,460,557,463 bytes of data, 2,095,636,143 bytes of indexes, and 611,244,970 bytes of retained versions; `files.lance` is 6,313,762,330 bytes, including 6,114,495,742 bytes of data and 199,163,580 bytes of indexes.
- Split shards exactly: 73,635 node JSON files occupy 2,725,720,996 bytes and 73,635 edge JSON files occupy 2,125,983,359 bytes. There are no embedding JSON sidecars in this reset store, so vectors do not explain these 4.85 GB.
- Counted UIDs through the Linux AST graph. For Comment, Function, Struct, Field, EnumMember, Variable, Value, Include, and Mapping, every UID is distinct within its label. UID interning/dictionary compression is therefore a poor primary optimization for this corpus.
- Identified two scalable duplications in the search schema. Each file source is stored verbatim in `source` and copied again into the FTS `body`; entity bodies duplicate names, split/lowercase forms, labels, docstrings, n-grams, and paths. This explains why the files Lance data alone is 3.7x the source tree.
- Confirmed a CPU-amplifying vector representation: every entity row includes a nullable fixed-size `FLOAT[768]`. Apache Arrow's `FixedSizeListBuilder.AppendNull` appends 768 child nulls for every missing vector. On approximately 6.9 million rows with no embeddings, the current rebuild executes roughly 5.3 billion child-null appends before Lance ingestion.
- Confirmed fragment/version amplification: `RebuildFromCache` appends only 2,000 rows per call, producing approximately 3,445 entity fragments/versions before indexes are built. Compaction is deferred until after FTS/scalar index construction, matching the separate 62.36-second maintenance phase and the 611 MB versions directory.
- Added a correctness test and benchmark for nullable vector batches. The original Arrow construction took 4.730 ms and allocated 17.46 MB per 2,000 all-null `FLOAT[768]` rows; bulk construction takes 1.547 ms and 14.86 MB, a 3.1x CPU improvement for that stage while preserving null and populated vector round trips.
- Increased the entity ingestion batch from 2,000 to 8,192 rows. On the Linux row count this reduces append-created fragments/versions from approximately 3,445 to approximately 842 while keeping the temporary dense vector buffer near 25 MB per batch.
- Added end-user search rebuild timing fields for setup, row preparation, file/entity writes, file/entity FTS, scalar indexes, and vector-status publication. The next Linux reset benchmark will attribute the previous opaque 606.82 seconds directly.
- Ran the first optimized Linux reset benchmark: parse 166.70s, write 324.88s, total 495.5s. Search build fell from 606.82s to 221.51s and total write fell from 712.31s to 324.88s. The new breakdown is setup 0.00s, preparation 118.29s, file write 8.73s, entity write 35.94s, file FTS 22.11s, file scalar 0.03s, entity FTS 33.54s, entity scalar 2.85s, publication 0.00s.
- Observed full-rebuild maintenance spending another 57.24s after the indexes were usable. Before maintenance, file and entity data occupied approximately 3.05 GB and 3.04 GB. Immediate compaction rewrote them to approximately 6.11 GB and 6.46 GB, but the 15-minute MVCC retention prevented pruning the originals. Because no later maintenance is scheduled solely to cross the retention boundary, the duplicate remains durable. Full rebuilds now skip maintenance; incrementals and vector finalization retain it because they create dead rows.
- Measured the 4,851,704,347-byte plain-JSON shard directory through zstd level 1: it compresses to 281,192,320 bytes (17.3x smaller) in 26.09s as one stream. Switched independent node/edge shard payloads to zstd-compressed JSON frames and bumped the cache version. Per-file compression will be measured by the second Linux reset; separate frames preserve O(1) loading and per-file incremental replacement.
- Ran the final Linux reset benchmark with compressed per-file shards and no full-rebuild compaction: parse 151.92s, write 270.58s, total 423.0s. Search build was 224.55s with preparation 122.92s, file/entity writes 8.71s/34.94s, file/entity FTS 21.81s/33.33s, scalar indexes 2.83s, and publication effectively zero. Search maintenance was zero.
- Measured the final store at 7,843,802,219 apparent bytes (8,287,318,016 allocated bytes), down from 20,583,557,177 apparent bytes: a 61.9% reduction. `search.lance` is now 7,265,877,230 bytes, compressed shards are 326,928,593 bytes (93.3% smaller / 14.8x), and `graph.icebug` remains approximately 240 MB.
- Final benchmark comparison: write 712.31s -> 270.58s (-62.0%), total 873.2s -> 423.0s (-51.6%), search build 606.82s -> 224.55s (-63.0%), and immediate full-rebuild maintenance 62.36s -> 0s. Compression did not regress parse (160.42s baseline -> 151.92s final) and added only approximately 3.0s to row preparation relative to the first optimized plain-JSON run.
- Verified all packages with `go test -count=1 -tags lancedb ./...`. Focused `go vet -tags lancedb ./internal/ast ./internal/lancestore ./cmd/graphit/commands` passed. Repository-wide vet still reports the pre-existing unreachable-code diagnostics in generated ANTLR parsers.

## Technical Debt

- Backlogged `Build the Lance search sidecar in staging while parsing shards`: the remaining 122.92s preparation plus 43.65s writes can overlap the 151.92s parse, but correctness requires generation-scoped staging and atomic graph/search publication.
- Backlogged `Remove the duplicate source-text copy from the Lance files table`: `files.lance` remains 3.16 GB because source text is present in both `source` and the FTS `body`; changing ownership must preserve `ast source`, file recall, and remote publication.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/profile-linux-corpus-index-time-and-storage.md` | Created | Keep benchmark evidence, decisions, implementation, and verification resumable. |
| `internal/lancestore/store_lancedb.go` | Modified | Build nullable fixed-size vector columns in bulk instead of executing 768 child-null appends per empty vector. |
| `internal/lancestore/fold_lancedb_test.go` | Modified | Cover null/value vector equivalence and retain a focused ingestion benchmark. |
| `internal/ast/search_lance.go` | Modified | Increase the bulk-load batch and expose granular rebuild timings. |
| `internal/ast/pipeline.go` | Modified | Carry granular search timings through the pipeline result. |
| `cmd/graphit/commands/runners.go` | Modified | Print the search timing breakdown for real-corpus diagnosis. |
| `internal/ast/shard_cache.go` | Modified | Store per-file node/edge JSON payloads as zstd frames and invalidate the old cache format. |
| `internal/ast/embedder_no_source_test.go` | Modified | Decompress the shard before asserting that source text and the legacy row are absent. |
