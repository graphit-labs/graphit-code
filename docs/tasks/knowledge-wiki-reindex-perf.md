# Wiki Reindex Performance Fix (knowledge + memory)

**Date:** 2026-06-18  
**Status:** Done

## Problem

O `graphit sync` / `graphit knowledge index` / `graphit memory index` demoravam bastante mesmo quando nada tinha mudado e tudo estava no cache. Os mesmos bottlenecks afetavam ambas as wikis.

## Root Causes

### 1. Fast-path O(N) file reads (knowledge e memory)

O fast-path verificava `content_hash` lendo cada arquivo `.md` do wikiDir do disco. Com 200 docs = 200 leituras de arquivo.

### 2. WikiDB rebuild sempre executava em `wiki/pipeline.go`

`RebuildDB` sempre fazia write-to-temp + FTS5 rebuild + atomic rename + embedding restore, mesmo quando nenhum doc mudou. Na wiki de memória, `logEntry` era **sempre** não-nil, então o fast-path do pipeline nunca disparava.

### 3. FTS5 `optimize` em cada rebuild

`optimizeTables()` (merge de segmentos FTS5) rodava em todo rebuild — operação cara e desnecessária.

## Fixes

### Fix 1 — Fast-path via `processCache` (zero disk I/O)

`internal/knowledge/wiki.go` e `internal/memory/wiki.go`: substituído o loop de N leituras de `.md` por `processCache.HasChanged()` — O(1) em memória por doc. Se o manifest do cache confirmar que nenhum arquivo mudou, retorna imediatamente sem tocar o disco.

### Fix 2 — Skip do rebuild via `CheckAllHashesMatch`

`internal/wiki/fts.go`: adicionado `CheckAllHashesMatch(chunks []WikiChunk) bool` — faz uma única query `SELECT content_hash FROM chunks` e compara em Go.

`internal/wiki/pipeline.go`: `RebuildDB` chama `CheckAllHashesMatch` antes do `Rebuild`. Se bater (e `logEntry == nil`), retorna imediatamente.

`internal/memory/wiki.go`: corrigido para passar `logEntry = nil` quando nenhum artigo foi escrito — habilita o skip no pipeline.

### Fix 3 — FTS5 `optimize` condicional

`internal/wiki/fts.go`: substituído `optimizeTables()` por `optimizeTablesIfNeeded()` — roda FTS optimize apenas a cada 10 rebuilds (contador em `wiki_meta.rebuild_count`, copiado entre rebuilds no atomic rename).

## Files Changed

- `internal/knowledge/wiki.go` — fast-path O(1) via processCache
- `internal/memory/wiki.go` — fast-path O(1) + logEntry condicional
- `internal/wiki/fts.go` — `CheckAllHashesMatch`, `optimizeTablesIfNeeded`, cópia de `wiki_meta` no Rebuild
- `internal/wiki/pipeline.go` — skip condicional do rebuild quando `logEntry == nil && CheckAllHashesMatch`

## Verification

- `go build ./internal/knowledge/... ./internal/wiki/... ./internal/memory/...` — OK
- `go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/...` — **PASS** (30s total)
