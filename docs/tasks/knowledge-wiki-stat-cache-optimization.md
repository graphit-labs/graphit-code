# Knowledge Wiki Sync: Parallel Stat Pre-Check (AST Pattern)

## Objective
Make `graphit sync` performant when nothing has changed, especially the "Knowledge wiki
reindexed" step.

## Result
- Before: 4.3s (even without changes)
- After: 0.0s ✓ (same as AST)

## Root cause
The wiki did a filepath.Walk over the ENTIRE project (585+ Go files) + ReadFile of 199 doc
files on every sync, even when nothing had changed.

## Solution implemented

### AST pattern (pipeline.go)
AST uses cached mtime+size. If changedFiles==0 → returns immediately. No Walk, no hash, nothing.

### Implementation in the wiki

**`internal/wiki/process_cache.go`**:
- Added `Mtime int64` (UnixNano), `Size int64` to `wikiCacheManifestEntry`
- `AllStatEntries()` → returns all files with mtime; nil if any has Mtime==0
- `StatMatch(relPath, mtime, size)` → O(1), returns (hash, true) if it matches
- `StoreMtime(relPath, mtime, size)` → persists mtime; MUST call `dirty[""] = true`

**`internal/knowledge/wiki.go`** (before the Walk):
```
Phase A: parallel stat of cached files
  → statMatch: mtime+size equal → skip
  → needHash: mtime/size different → needs hash
Phase B: ReadFile+hash only for needHash
  → hash equal → StoreMtime → allUnchanged
  → hash different → allUnchanged = false → full Walk
If allUnchanged && wiki.db exists → Save() → return
```

### Critical bugs fixed
1. `StoreMtime` without `dirty[""] = true` → mtime doesn't persist to disk → pre-check never activates
2. `Unix()` instead of `UnixNano()` → insufficient precision, mtime always different
3. Using `FastPathCheck` in the pre-check → fails for chunked markdown (multiple slugs per source file)
4. `StoreMtime` must be called after a Walk stat-cache HIT AND after `Store()` new processing

## Modified Files
- `internal/wiki/process_cache.go`
- `internal/knowledge/wiki.go`
- `internal/knowledge/knowledgeignore.go` (added .agents/, .claude/ etc to the ignore list, renamed to .wikiignore)
