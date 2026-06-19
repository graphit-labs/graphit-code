# Knowledge Wiki Sync: Parallel Stat Pre-Check (AST Pattern)

## Objetivo
Tornar o `graphit sync` performático quando nada mudou, especialmente o step "Knowledge wiki reindexed".

## Resultado
- Antes: 4.3s (mesmo sem mudanças)
- Depois: 0.0s ✓ (igual ao AST)

## Causa raiz
O wiki fazia filepath.Walk sobre TODO o projeto (585+ Go files) + ReadFile de 199 doc files a cada sync, mesmo sem nada ter mudado.

## Solução implementada

### Padrão AST (pipeline.go)
O AST usa mtime+size em cache. Se changedFiles==0 → retorna imediatamente. Sem Walk, sem hash, sem nada.

### Implementação no wiki

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

### Bugs críticos corrigidos
1. `StoreMtime` sem `dirty[""] = true` → mtime não persiste no disco → pre-check nunca ativa
2. `Unix()` em vez de `UnixNano()` → precisão insuficiente, mtime sempre diferente
3. Usar `FastPathCheck` no pre-check → falha para markdown chunked (múltiplos slugs por source file)
4. `StoreMtime` deve ser chamado após Walk stat-cache HIT E após `Store()` novo processamento

## Arquivos modificados
- `internal/wiki/process_cache.go`
- `internal/knowledge/wiki.go`
- `internal/knowledge/knowledgeignore.go` (adicionado .agents/, .claude/ etc ao ignore)
