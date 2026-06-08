# Fix Unbounded Memory Growth in `graphit ast index`

## Summary
Fixed three root causes of unbounded memory growth during AST indexing:
1. ShardCache accumulation without intermediate flushing
2. Entity source duplication from parent nodes
3. Bulk-loading all shards during rebuild

## Files Changed
- `internal/ast/shard_cache.go` — FlushDirty(), StreamEntries(), flushLocked()
- `internal/ast/pipeline.go` — periodic flush every 100 files, nil ParsedFile after cache
- `internal/ast/parser.go` — capEntitySource() helper (4KB max)
- `internal/ast/antlr_adapter.go` — cap entity source
- `internal/ast/treesitter_adapter.go` — cap entity source
- `internal/ast/json_rebuild.go` — StreamEntries() instead of AllEntries()

## Status
- [x] Implementation complete
- [x] `make ci` passes
