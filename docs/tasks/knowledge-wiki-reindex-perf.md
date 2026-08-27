# Wiki Reindex Performance Fix (knowledge + memory)

**Date:** 2026-06-18  
**Status:** Done

## Problem

O `graphit sync` / `graphit knowledge index` / `graphit memory index` demoravam bastante mesmo quando nada tinha mudado e tudo estava no cache. Os mesmos bottlenecks afetavam ambas as wikis.

## Root Causes

### 1. Fast-path O(N) file reads (knowledge e memory)

O fast-path verificava `content_hash` lendo cada arquivo `.md` do wikiDir do disco. Com 200 docs = 200 leituras de arquivo.

### 2. WikiDB rebuild sempre executava em `wiki/pipeline.go`

The inline 0 always performed write-to-temp + FTS5 rebuild + atomic rename + embedding restore, even when no document had changed. On the memory wiki, inline 1 was always not null, so the fast path of the pipeline never triggered.

### 3. FTS5 `optimize` em cada rebuild

The `optimizeTables()` (FTS5 segments merge) operation ran on every rebuild — a costly and unnecessary task.

## Fixes

### Fix 1 — Fast-path via `processCache` (zero disk I/O)

Inline 0 and Inline 1: Replaced the N reads of Inline 2 with Inline 3 — O(1) in memory by design. If the cache manifest confirms that no file has changed, it returns immediately without touching the disk.

### Fix 2 — Skip do rebuild via `CheckAllHashesMatch`

Added `CheckAllHashesMatch(chunks []WikiChunk) bool` — performs a single query `SELECT content_hash FROM chunks` and compares in Go.

`internal/wiki/pipeline.go`: `RebuildDB` chama `CheckAllHashesMatch` antes do `Rebuild`. Se bater (e `logEntry == nil`), retorna imediatamente.

`internal/memory/wiki.go`: corrigido para passar `logEntry = nil` quando nenhum artigo foi escrito — habilita o skip no pipeline.

### Fix 3 — FTS5 `optimize` condicional

The inline replacement of `optimizeTables()` with `optimizeTablesIfNeeded()` is optimized for every 10 rebuilds (`wiki_meta.rebuild_count`).

## Files Changed

- `internal/knowledge/wiki.go` — fast-path O(1) via processCache
- `internal/memory/wiki.go` — fast-path O(1) + logEntry condicional
- Copy of `wiki_meta` in `optimizeTablesIfNeeded` during `CheckAllHashesMatch` and `internal/wiki/fts.go`
- `internal/wiki/pipeline.go` — skip condicional do rebuild quando `logEntry == nil && CheckAllHashesMatch`

## Verification

- `go build ./internal/knowledge/... ./internal/wiki/... ./internal/memory/...` — OK
- `go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/...` — **PASS** (30s total)
