# Per-Grammar ANTLR Sidecar Implementation

## Status: ✅ Complete

## Summary

Implemented per-grammar ANTLR sidecar binaries packaged as `.grammar` fat archives.
Each grammar is now an independent, plug-and-play binary that can be distributed separately.

## Changes Made

### Step 1: Per-Grammar Driver Files
Created build-tag-controlled driver registration files:
- `cmd/graphit-antlr-sidecar/driver_plsql.go` — `//go:build grammar_plsql`
- `cmd/graphit-antlr-sidecar/driver_postgresql.go` — `//go:build grammar_postgresql`
- `cmd/graphit-antlr-sidecar/driver_tsql.go` — `//go:build grammar_tsql`
- `cmd/graphit-antlr-sidecar/driver_db2.go` — `//go:build grammar_db2`
- `cmd/graphit-antlr-sidecar/driver_cobol85.go` — `//go:build grammar_cobol85`
- `cmd/graphit-antlr-sidecar/driver_all.go` — fallback (no per-grammar tag)

### Step 2: Main.go Refactored
Removed hardcoded driver imports and map initialization. The `drivers` map is now populated
by `init()` functions in the per-grammar files.

### Step 3: antlr_adapter.go — Per-Grammar Discovery
`initAntlrDrivers()` now:
1. Searches for per-grammar `.grammar` archives and sidecar binaries
2. Falls back to monolithic sidecar binary for backward compatibility
3. Search dirs: `~/.graphit/grammars/antlr/`, runtime dir, alongside binary

New functions: `findAntlrGrammarBin()`, `extractAntlrFromArchive()`, `antlrGrammarSearchDirs()`

### Step 4: Build Script
Build via `make grammars-antlr` (Makefile target, replaces scripts/build-antlr-grammars.sh).

## Binary Size Report

| Grammar    | Per-Grammar (stripped) | Per-Grammar (unstripped) | .grammar Archive |
|------------|----------------------|--------------------------|------------------|
| PL/SQL     | 22M                  | 31M                      | 4.1M             |
| PostgreSQL | 13M                  | —                        | 2.6M             |
| T-SQL      | 16M                  | —                        | 3.1M             |
| DB2        | 17M                  | —                        | 3.2M             |
| COBOL85    | 11M                  | —                        | 2.3M             |
| **Total**  | **79M**              | —                        | **15.3M**        |
| Monolithic | 67M (stripped)       | —                        | —                |

**Key insight**: Sum of per-grammar (79M) > monolithic (67M) due to shared Go runtime in each binary.
But .grammar archives compress to just 15.3M total — better than monolithic for distribution.

## Benchmark Results (PL/SQL, 3s × 3 runs)

| Approach                  | ns/op    | B/op    | allocs/op |
|---------------------------|----------|---------|-----------|
| Per-Grammar Sidecar       | ~414k    | 70,232  | 552       |
| Per-Grammar Sidecar Pool  | ~150k    | 70,234  | 552       |
| Native In-Process         | ~930k    | 625,553 | 9,773     |

**Performance notes**:
- Per-grammar sidecar is identical in performance to monolithic sidecar (both use same protocol)
- Pooled sidecar is **6.2× faster** than native in-process (150k vs 930k ns/op)
- Sidecar uses **8.9× less memory** per parse (70KB vs 625KB)
- The sidecar avoids ANTLR's Go runtime overhead by pre-serializing the parse tree

## Verification
- `go build -tags fts5 ./...` — ✅ Clean compilation
- All 5 per-grammar binaries built successfully
- All 5 `.grammar` archives created and verified
- Benchmarks pass with no regressions
