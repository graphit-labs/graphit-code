# Task: Grammar Archive Format (.grammar)

## Status: Complete

## Summary

Implemented a cross-platform `.grammar` fat archive format that packages multiple platform-specific shared libraries into a single distributable file with zstd compression.

## Files Created/Modified

### Created
- `internal/ast/grammar_archive.go` — Archive format reader/writer with zstd compression
- `internal/ast/grammar_archive_test.go` — 7 tests + 5 benchmarks
- `cmd/graphit-grammar-pack/main.go` — CLI tool for creating archives

### Modified
- `internal/ast/treesitter_dynload.go` — DynGrammarLoader with CGO dlopen/dlsym (simplified, no archive loading)

### Generated Artifacts
- `.build/grammars/treesitter/tree-sitter-go.grammar` — Go grammar archive (43KB from 221KB .so)
- `.build/grammars/treesitter/tree-sitter-python.grammar` — Python grammar archive (78KB from 484KB .so)

## Archive Format

Binary format with GRMT magic, zstd-compressed platform payloads:
- Header: 16 bytes (magic + version + count + flags)
- Entries: N × 120 bytes each (OS/Arch/Symbol + offset/sizes)
- Data: concatenated zstd-compressed shared libraries

## Compression Results

| Grammar | Original (.so) | Archive (.grammar) | Ratio |
|---------|---------------|-------------------|-------|
| Go      | 220,808 bytes | 42,727 bytes      | 19.3% |
| Python  | 483,592 bytes | 77,665 bytes      | 16.1% |

## Benchmark Results

Parse performance is identical across all loading paths (within noise):

| Method | ns/op | B/op | allocs/op |
|--------|-------|------|-----------|
| Native CGO import | ~3,200 | 8,440 | 5 |
| Shared lib (CGO dlopen) | ~3,200 | 8,440 | 5 |
| Grammar archive (cached) | ~3,200 | 8,440 | 5 |

One-time extraction cost: ~32ms per grammar (amortized to zero on subsequent loads).
