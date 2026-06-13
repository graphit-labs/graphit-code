# Plug-and-Play Grammar Architecture

## Status: Complete

## Summary
Gramáticas Tree-sitter e ANTLR migradas de compilação estática para carregamento dinâmico plug-and-play.
Formato `.grammar` (GRMT fat archive) distribui um único arquivo cross-platform por gramática.

## Architecture
- **Tree-sitter**: shared libraries (.so/.dylib/.dll) carregadas via CGO dlopen/dlsym
- **ANTLR**: sidecar binaries standalone com IPC stdin/stdout length-prefixed
- **Build**: `make grammars-treesitter` (37 gramáticas), `make grammars-antlr` (5 sidecars)
- **Distribution**: Hub artifact type `language` (YAML + .grammar archive)
- **Launcher**: embute 16 TS defaults, ANTLR via Hub

## Format: `.grammar` (GRMT v1)
- Header 16B + N × 120B platform entries + zstd compressed data
- Um único arquivo por gramática, contém binários de todas as plataformas
- Extração at install time (Hub) ou build time (Launcher)
- Compressão: ~91% (50MB .so → 4.4MB .grammar para 37 gramáticas)

## Performance (valores reais medidos)
- Tree-sitter CGO dlopen: +14% overhead vs nativo (dentro da margem de ruído)
- ANTLR sidecar pooled: 6.2x mais rápido que in-process no client-side
- Memória: idêntica ao nativo para Tree-sitter
- ANTLR: 89% menos alocações no client

## Search Path Hierarchy
1. Projeto: `.graphit/grammars/{treesitter,antlr}/`
2. Global: `~/.graphit/grammars/{treesitter,antlr}/`
3. Runtime: `~/.graphit/runtime/<version>/grammars/{treesitter,antlr}/`

## Files
- `internal/ast/grammar_archive.go` — formato GRMT reader/writer
- `internal/ast/treesitter_dynload.go` — DynGrammarLoader (CGO dlopen/dlsym)
- `internal/ast/antlr_sidecar.go` — SidecarDriver com process pool
- `internal/ast/antlr_adapter.go` — ANTLR adapter com project-level search
- `internal/hub/grammar_install.go` — extração de .grammar no hub install
- `cmd/graphit-grammar-pack/main.go` — CLI para criar .grammar archives
- `cmd/graphit-antlr-sidecar/main.go` — ANTLR sidecar binary
- `Makefile` — targets: grammars, grammars-treesitter, grammars-antlr, grammars-clean
