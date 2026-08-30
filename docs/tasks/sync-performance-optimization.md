# Task: Sync Performance Optimization

**Date:** 2026-06-18  
**Status:** ✅ Complete

## Problem

`graphit sync` was slow even when nothing had changed in the project:
- **Baseline:** wall=57s, user=22.8s CPU, sys=2.8s

The time was dominated by 4 bottlenecks:

1. **AST**: SHA-256 of every file on each sync, even without changes
2. **Knowledge wiki**: `autoLinkContent` and `resolveWikiLinksInBody` ran for every doc before any cache check; whole-page string comparison failed because of the `updated: <today>` field that changes daily
3. **Memory wiki**: same pattern — no early-exit before the loop
4. **`installAllRules`**: rewrote AGENTS.md, SKILL.md, MCP config and all rule files every time, even without changes → 195,880 file system outputs per sync

## Solution

### 1. AST: mtime pre-filter ([`shard_cache.go`](../../internal/ast/shard_cache.go), [`pipeline.go`](../../internal/ast/pipeline.go))

- Added `Mtime int64` field to `shardManifestEntry`
- Methods `NeedsHash(relPath, mtime)` and `StoreMtime(relPath, mtime)`
- In the pipeline: parallel `stat()` of all files → only hash the ones with a different mtime
- After a successful parse, it stores the mtime → the next sync is even faster

### 2. Knowledge wiki: early-exit before the expensive phases ([`knowledge/wiki.go`](../../internal/knowledge/wiki.go))

- **Pre-scan** of `content_hash` in the frontmatter of existing pages **before** `buildAutoLinkTargets`, `autoLinkContent`, `resolveWikiLinksInBody`
- If all hashes match and `wiki.db` exists → `return result, nil` immediately
- Added `readKnowledgeFrontmatterField` helper (fast read without a full parse)
- Eliminated the false positive from `updated: <today>` that invalidated the cache daily

### 3. Memory wiki: pre-scan ([`memory/wiki.go`](../../internal/memory/wiki.go))

- `content_hash` added to page frontmatter via `memoryEntityPageWithHash`
- Pre-scan of hashes + stale-page check before the generation loop
- Skip `wiki.RebuildDB` when nothing changed
- Added `readFrontmatterField` helper

### 4. `installAllRules`: idempotence ([`mandate.go`](../../internal/hub/adapters/ide/mandate.go), [`adapters.go`](../../internal/hub/adapters/ide/adapters.go), [`base.go`](../../internal/hub/adapters/ide/base.go))

- `UpsertMandateTrigger`: early check — if the trigger content is identical, returns without touching the file
- `installSkillForAdapter`: compares SKILL.md content before `WriteFile`
- `copyFile`: compares size + content before copying
- `reconcileMCPFile`: compares the resulting JSON before `WriteFile`

## Result

| Metric | Before | After | Gain |
|---------|-------|--------|-------|
| Wall time | 57s | **30s** | **2x faster** |
| User CPU | 22.8s | **5.3s** | **4.3x less CPU** |
| Sys (I/O) | 2.8s | **1.1s** | **2.5x less I/O** |

The remaining 30s are network (git pull of the memory repo + hub repo) — irreducible without a network cache.

## Modified Files

- `internal/ast/shard_cache.go` — Mtime field + NeedsHash/StoreMtime
- `internal/ast/pipeline.go` — mtime pre-filter in the parallel pipeline
- `internal/knowledge/wiki.go` — hash pre-scan + early-exit + readKnowledgeFrontmatterField
- `internal/memory/wiki.go` — hash pre-scan + content_hash in frontmatter + readFrontmatterField
- `internal/hub/adapters/ide/mandate.go` — idempotence in UpsertMandateTrigger
- `internal/hub/adapters/ide/adapters.go` — idempotence in installSkillForAdapter
- `internal/hub/adapters/ide/base.go` — idempotence in copyFile and reconcileMCPFile
