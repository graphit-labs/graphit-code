# Wiki Reindex Performance Fix (knowledge + memory)

**Date:** 2026-06-18  
**Status:** Done

## Problem

`graphit sync` / `graphit knowledge index` / `graphit memory index` were quite slow even when
nothing had changed and everything was in the cache. The same bottlenecks affected both wikis.

## Root Causes

### 1. Fast-path O(N) file reads (knowledge and memory)

The fast-path checked `content_hash` by reading every `.md` file in wikiDir from disk. With 200
docs = 200 file reads.

### 2. WikiDB rebuild always ran in `wiki/pipeline.go`

`RebuildDB` always did write-to-temp + FTS5 rebuild + atomic rename + embedding restore, even
when no doc had changed. On the memory wiki, `logEntry` was **always** non-nil, so the
pipeline's fast path never triggered.

### 3. FTS5 `optimize` on every rebuild

`optimizeTables()` (FTS5 segment merge) ran on every rebuild — an expensive and unnecessary
operation.

## Fixes

### Fix 1 — Fast-path via `processCache` (zero disk I/O)

`internal/knowledge/wiki.go` and `internal/memory/wiki.go`: replaced the loop of N `.md` reads
with `processCache.HasChanged()` — O(1) in memory per doc. If the cache manifest confirms no
file has changed, it returns immediately without touching disk.

### Fix 2 — Skip the rebuild via `CheckAllHashesMatch`

`internal/wiki/fts.go`: added `CheckAllHashesMatch(chunks []WikiChunk) bool` — runs a single
`SELECT content_hash FROM chunks` query and compares in Go.

`internal/wiki/pipeline.go`: `RebuildDB` calls `CheckAllHashesMatch` before `Rebuild`. If it
matches (and `logEntry == nil`), it returns immediately.

`internal/memory/wiki.go`: fixed to pass `logEntry = nil` when no article was written — enables
the skip in the pipeline.

### Fix 3 — Conditional FTS5 `optimize`

`internal/wiki/fts.go`: replaced `optimizeTables()` with `optimizeTablesIfNeeded()` — runs FTS
optimize only every 10 rebuilds (counter in `wiki_meta.rebuild_count`, copied between rebuilds
in the atomic rename).

## Files Changed

- `internal/knowledge/wiki.go` — fast-path O(1) via processCache
- `internal/memory/wiki.go` — fast-path O(1) + conditional logEntry
- `internal/wiki/fts.go` — `CheckAllHashesMatch`, `optimizeTablesIfNeeded`, copy of `wiki_meta` in Rebuild
- `internal/wiki/pipeline.go` — conditional skip of rebuild when `logEntry == nil && CheckAllHashesMatch`

## Verification

- `go build ./internal/knowledge/... ./internal/wiki/... ./internal/memory/...` — OK
- `go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/...` — **PASS** (30s total)
