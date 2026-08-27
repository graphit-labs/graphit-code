# Task: Sync Performance Optimization

**Date:** 2026-06-18  
**Status:** ✅ Complete

## Problema

`graphit sync` estava lento mesmo quando nada mudava no projeto:
- **Baseline:** wall=57s, user=22.8s CPU, sys=2.8s

O tempo era dominado por 4 gargalos:

"AST": Hash all files on every sync, even without changes.
2. **Knowledge Wiki:** Inline 0 and Inline 1 were previously serving all documents before any cache check; the full page comparison failed due to the Inline 2 that changes daily.
3. **Memory Wiki:** Same format — no early exit before the loop
Step 1: Identify the context and key elements of the Portuguese text.
The text appears to be discussing a process involving multiple files being rewritten or updated periodically.

Step 2: Translate each part while maintaining the overall meaning and structure:
- "4. `installAllRules`": This could refer to a specific step number, so it remains unchanged.
"Re-wrote AGENTS.md, SKILL.md, MCP configuration, and all rule files every time, even without changes → 195,880 filesystem output syncs"
  - "re-writing" is translated to "rewriting."
  - "even without changes" is translated to "even without modifications."

Step 3: Combine the translated parts into a coherent English sentence:
4. `installAllRules`: re-wrote AGENTS.md, SKILL.md, MCP config and all rule files every time, even without changes → 195.880 file system outputs per sync

Final translation:
4. `installAllRules`: rewrote AGENTS.md, SKILL.md, MCP config and all rule files every time, even without changes → 195.880 file system outputs per sync

Solution

### 1. AST: mtime pre-filter ([`shard_cache.go`](../../internal/ast/shard_cache.go), [`pipeline.go`](../../internal/ast/pipeline.go))

- Adicionado campo `Mtime int64` em `shardManifestEntry`
Methods INLINE 0 and INLINE 1
No pipeline: inline parallel of all files → only hash on files with different mtime
After a successful parse, it stores mtime → next sync is even faster.

### 2. Knowledge wiki: early-exit antes das fases caras ([`knowledge/wiki.go`](../../internal/knowledge/wiki.go))

Pre-check of `content_hash` in the existing pages' front matter before `buildAutoLinkTargets`, `autoLinkContent`, and __INLINE_3.
- Se todos os hashes batem e `wiki.db` existe → `return result, nil` imediatamente
Added inline helper (quick read without full parsing)
- Eliminado falso-positivo do `updated: <today>` que invalidava cache diariamente

### 3. Memory wiki: pre-scan ([`memory/wiki.go`](../../internal/memory/wiki.go))

Added inline to the front matter of pages via inline
Pre-scanning of hashes and checking for stale pages before the generation loop
- Skip de `wiki.RebuildDB` quando nada mudou
- Adicionado `readFrontmatterField` helper

4. Idempotence ([`mandate.go`](../../internal/hub/adapters/ide/mandate.go), [`adapters.go`](../../internal/hub/adapters/ide/adapters.go), [`base.go`](../../internal/hub/adapters/ide/base.go))

Brazilian Portuguese:
- `UpsertMandateTrigger`: check early — if the trigger content is identical, return without touching the file
Brazilian Portuguese to idiomatic English:

- Compare content of SKILL.md before __INLINE_1
- INLINE 0: compares size and content before copying
- `reconcileMCPFile`: compara JSON resultante antes de `WriteFile`

## Resultado

Metric | Before | After | Gain
|---------|-------|--------|-------|
Wall Time: 57s | **30s** | **2x Faster**
| User CPU | 22.8s | **5.3s** | **4.3x menos CPU** |
| Sys (I/O) | 2.8s | **1.1s** | **2.5x menos I/O** |

The remaining 30 seconds are rede (pull from git memory repository and hub repository) – irreducible without network cache.

## Arquivos Modificados

- `internal/ast/shard_cache.go` — campo Mtime + NeedsHash/StoreMtime
- `internal/ast/pipeline.go` — pre-filtro mtime no pipeline paralelo
- `internal/knowledge/wiki.go` — pre-scan hash + early-exit + readKnowledgeFrontmatterField
- `internal/memory/wiki.go` — pre-scan hash + content_hash no frontmatter + readFrontmatterField
Idempotence in UpsertMandateTrigger
Idempotence in installing Skill for Adapter
IDEMPOTENCE IN COPYFILE AND RECONCILE MCP FILE
