# Task: Native Grammar Compilation

## Objetivo
Compilar todas as 37 grammars tree-sitter e 5 ANTLR grammars diretamente no binário core, eliminando:
- Compilação C separada de shared libraries (.so/.dylib/.dll)
- Bundle + gzip + embed no launcher
- Extração em runtime (extractRuntime)
- dlopen/dlsym (DynGrammarLoader)
- Subprocessos sidecar (SidecarDriver)

## Mudanças

### Novos arquivos
- `internal/ast/treesitter_native.go` — registro de 39 grammars nativos com mapa `string → GetLanguage()`
- `internal/ast/treesitter/json/binding.go` — binding local para tree-sitter-json
- `internal/ast/treesitter/xml/binding.go` — binding local para tree-sitter-xml
- `internal/ast/treesitter/zig/binding.go` — binding local para tree-sitter-zig
- `internal/ast/treesitter/haskell/binding.go` — binding local para tree-sitter-haskell
- `internal/ast/treesitter/julia/binding.go` — binding local para tree-sitter-julia

### Arquivos modificados
- `internal/ast/treesitter_adapter.go` — `parseWithConfig()` agora chama `NativeLanguage()` primeiro, fallback para `DynGrammarLoader` (Hub custom grammars)
- `internal/ast/antlr_adapter.go` — substituiu SidecarDriver por drivers in-process (`plsql.Driver{}`, etc.)
- `Makefile` — removido grammar compilation/bundling dos build targets
- `go.mod` — adicionadas deps: tree-sitter-json, tree-sitter-xml, tree-sitter-zig, tree-sitter-haskell, tree-sitter-julia

## Resultados de benchmark

### Tree-sitter: LangLookup Native vs Dynamic (cached)
- **Native**: ~50ns/op (CGO call)
- **Dynamic**: ~14ns/op (sync.Map cache hit)
- Diferença irrelevante no parse total (~800μs)

### Tree-sitter: Full Parse (com pool, via nativo)
- Go (~200 LOC): ~855μs, 5.34 MB/s
- Python (~200 LOC): ~995μs, 5.53 MB/s
- JavaScript (~200 LOC): ~872μs, 5.03 MB/s

### ANTLR: In-process (sem sidecar)
- PL/SQL (~100 LOC): ~1.05ms, 2.49 MB/s
- PostgreSQL (~80 LOC): ~829μs, 2.82 MB/s

### Launcher size
- Core binary: 239MB (todas as grammars compiladas)
- Launcher: **3.2MB** (sem grammars embarcados)
