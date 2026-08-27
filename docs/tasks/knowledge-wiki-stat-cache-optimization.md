# Knowledge Wiki Sync: Parallel Stat Pre-Check (AST Pattern)

## Objetivo
Make INLINE 0 performant when nothing has changed, especially with step "Knowledge Wiki Reindex".

## Resultado
Before: 4.3 seconds (unchanged)
- Depois: 0.0s ✓ (igual ao AST)

## Causa raiz
O wiki fazia filepath.Walk sobre TODO o projeto (585+ Go files) + ReadFile de 199 doc files a cada sync, mesmo sem nada ter mudado.

Solution implemented

Pipeline Pattern (pipeline.go)
O AST usa mtime+size em cache. Se changedFiles==0 → retorna imediatamente. Sem Walk, sem hash, sem nada.

Implementation on the wiki

**`internal/wiki/process_cache.go`**:
- Adicionado `Mtime int64` (UnixNano), `Size int64` em `wikiCacheManifestEntry`
- `AllStatEntries()` → retorna todos os arquivos com mtime; nil se algum tiver Mtime==0
- `StatMatch(relPath, mtime, size)` → O(1), retorna (hash, true) se bater
- `StoreMtime(relPath, mtime, size)` → persiste mtime; DEVE chamar `dirty[""] = true`

**`internal/knowledge/wiki.go`** (antes do Walk):
```
Phase A: stat paralelo dos arquivos em cache
  → statMatch: mtime+size igual → skip
  → needHash: mtime/size diferente → precisa hash
Phase B: ReadFile+hash apenas needHash
  → hash igual → StoreMtime → allUnchanged
  → hash diferente → allUnchanged = false → full Walk
Se allUnchanged && wiki.db existe → Save() → return
```

Critical bugs fixed
Here is the idiomatic English translation:

Without `StoreMtime`, mtime does not persist on disk → pre-check never activates
"Replace INLINE_0 with INLINE_1 → insufficient precision, always differentmtime."
Use `INLINE_0` in the pre-check → fails for Markdown chunked (multiple slugs per source file)
4. It should be called after Walk Stat-Cache HIT and after `Store()` new processing.

## Arquivos modificados
- `internal/wiki/process_cache.go`
- `internal/knowledge/wiki.go`
- `internal/knowledge/knowledgeignore.go` (adicionado .agents/, .claude/ etc ao ignore, renomeado para .wikiignore)
