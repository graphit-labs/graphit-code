# Remove the hidden source copy in AST shard `file_row`

## Objective

Remove the remaining source-text copy from AST `nodes.json` shards. The cache must keep
only the parse material needed to rebuild the graph and search index; source text comes from
the hash-validated working tree locally and from the published Lance index for Hub contexts.

## Reasoning

The earlier source-removal work removed the dedicated shard source field, but
`parseCacheEntry.FileRow` still serializes `src` in a positional array whenever
`IndexSource` is enabled. Its only observed shard rehydration purpose is carrying the optional
cluster at array index 6; all remaining File-node information already has explicit fields.

The project is in development, so the shard format will be version-bumped and rebuilt rather
than migrated.

## Plan

- [x] Replace the serialized positional `FileRow` with explicit cache metadata needed after a reload.
- [x] Remove obsolete source and file-row state from the parse cache conversion path.
- [x] Add a regression test that indexes source and proves its marker is absent from the on-disk shard.
- [x] Run focused AST tests and commit only task-owned files to `main`.

## Progress Log

### 2026-08-30 — task opened

- Confirmed the working tree has unrelated skill edits; they will remain unstaged.
- Confirmed the source leak is reproducible by code path: `ConvertToCache` puts `src` into
  `FileRow`, and `splitEntry` serializes it into the nodes shard.

### 2026-08-30 — implementation

- Replaced `shardNodes.FileRow` with `Cluster`; merge now reconstructs the explicit cluster
  field rather than decoding a positional tuple.
- Removed the source-reading and file-row construction branches in `ConvertToCache`; its
  transient fields remain for test-fixture compatibility but are no longer populated or persisted.
- Bumped `shardCacheVersion` from 10 to 11 so existing shards are discarded and reparsed.
- Extended the shard regression through `ConvertToCache(..., true, ...)`, checking that source
  text and `file_row` are absent while cluster metadata survives.

### 2026-08-31 — test fix and completion

- Updated `TestCompactionPreservesEveryValue` and `compactionCorpus` in `internal/ast/shard_compact_test.go` to reflect that `FileRow` is no longer stored or rehydrated in shards, verifying round-trip equivalence with explicit `Cluster`.
- Verified all AST tests pass cleanly.
