# Task: Sync Performance Optimization

**Date:** 2026-06-18  
**Status:** ✅ Complete

## Problema

`graphit sync` estava lento mesmo quando nada mudava no projeto:
- **Baseline:** wall=57s, user=22.8s CPU, sys=2.8s

O tempo era dominado por 4 gargalos:

1. **AST**: SHA-256 de todos os arquivos a cada sync, mesmo sem mudanças
2. **Knowledge wiki**: `autoLinkContent` e `resolveWikiLinksInBody` rodavam para todos os docs antes de qualquer check de cache; comparação de string da página inteira falhava por causa do `updated: <today>` que muda todo dia
3. **Memory wiki**: mesmo padrão — sem early-exit antes do loop
4. **`installAllRules`**: re-escrevia AGENTS.md, SKILL.md, MCP config e todos os arquivos de regras toda vez, mesmo sem mudanças → 195.880 file system outputs por sync

## Solução

### 1. AST: mtime pre-filter ([`shard_cache.go`](../../internal/ast/shard_cache.go), [`pipeline.go`](../../internal/ast/pipeline.go))

- Adicionado campo `Mtime int64` em `shardManifestEntry`
- Métodos `NeedsHash(relPath, mtime)` e `StoreMtime(relPath, mtime)`
- No pipeline: `stat()` paralelo de todos os arquivos → só hasheamos os com mtime diferente
- Após parse bem-sucedido, armazena mtime → próximo sync ainda mais rápido

### 2. Knowledge wiki: early-exit antes das fases caras ([`knowledge/wiki.go`](../../internal/knowledge/wiki.go))

- **Pre-scan** de `content_hash` no frontmatter das páginas existentes **antes** de `buildAutoLinkTargets`, `autoLinkContent`, `resolveWikiLinksInBody`
- Se todos os hashes batem e `wiki.db` existe → `return result, nil` imediatamente
- Adicionado `readKnowledgeFrontmatterField` helper (leitura rápida sem parse completo)
- Eliminado falso-positivo do `updated: <today>` que invalidava cache diariamente

### 3. Memory wiki: pre-scan ([`memory/wiki.go`](../../internal/memory/wiki.go))

- `content_hash` adicionado ao frontmatter das páginas via `memoryEntityPageWithHash`
- Pre-scan de hashes + check de stale pages antes do loop de geração
- Skip de `wiki.RebuildDB` quando nada mudou
- Adicionado `readFrontmatterField` helper

### 4. `installAllRules`: idempotência ([`mandate.go`](../../internal/hub/adapters/ide/mandate.go), [`adapters.go`](../../internal/hub/adapters/ide/adapters.go), [`base.go`](../../internal/hub/adapters/ide/base.go))

- `UpsertMandateTrigger`: check precoce — se trigger content é idêntico, retorna sem tocar o arquivo
- `installSkillForAdapter`: compara conteúdo de SKILL.md antes de `WriteFile`
- `copyFile`: compara size + conteúdo antes de copiar
- `reconcileMCPFile`: compara JSON resultante antes de `WriteFile`

## Resultado

| Métrica | Antes | Depois | Ganho |
|---------|-------|--------|-------|
| Wall time | 57s | **30s** | **2x mais rápido** |
| User CPU | 22.8s | **5.3s** | **4.3x menos CPU** |
| Sys (I/O) | 2.8s | **1.1s** | **2.5x menos I/O** |

Os 30s restantes são rede (git pull memory repo + hub repo) — irreducíveis sem cache de rede.

## Arquivos Modificados

- `internal/ast/shard_cache.go` — campo Mtime + NeedsHash/StoreMtime
- `internal/ast/pipeline.go` — pre-filtro mtime no pipeline paralelo
- `internal/knowledge/wiki.go` — pre-scan hash + early-exit + readKnowledgeFrontmatterField
- `internal/memory/wiki.go` — pre-scan hash + content_hash no frontmatter + readFrontmatterField
- `internal/hub/adapters/ide/mandate.go` — idempotência em UpsertMandateTrigger
- `internal/hub/adapters/ide/adapters.go` — idempotência em installSkillForAdapter
- `internal/hub/adapters/ide/base.go` — idempotência em copyFile e reconcileMCPFile
