# Task: Native Grammar Compilation

Objective:
Compile all 37 Tree-Sitter grammars and 5 ANTLR grammars directly into the core binary, eliminating:

- Separate compilation of C code with shared libraries (.so/.dylib/.dll)
- Bundle + gzip + embed in the launcher
- Runtime extraction (extractRuntime)
- dlopen/dlsym (DynGrammarLoader) 
- Sidecar processes (SidecarDriver)

Changes

### New Files
- `internal/ast/treesitter_native.go` — record of 39 native grammars with map `string → GetLanguage()`
- `internal/ast/treesitter/json/binding.go` — binding for tree-sitter-json
- `internal/ast/treesitter/xml/binding.go` — binding for tree-sitter-xml
- `internal/ast/treesitter/zig/binding.go` — binding for tree-sitter-zig
- `internal/ast/treesitter/haskell/binding.go` — binding for tree-sitter-haskell
- `internal/ast/treesitter/julia/binding.go` — binding for tree-sitter-julia

Modified files:
- `internal/ast/treesitter_adapter.go` — `parseWithConfig()` now calls `NativeLanguage()` first, falling back to `DynGrammarLoader` (custom grammars hub)
- `internal/ast/antlr_adapter.go` — replaced SidecarDriver with in-process drivers (`plsql.Driver{}`, etc.)
- `Makefile` — removed grammar compilation/bundling from build targets
- `go.mod` — added deps: tree-sitter-json, tree-sitter-xml, tree-sitter-zig, tree-sitter-haskell, tree-sitter-julia

Results of benchmark

Tree-sitter: LangLookup Native vs Dynamic (cached) - 
- **Native**: ~50ns/op (CGO call)
- **Dynamic**: ~14ns/op (sync.Map cache hit)
- Irrelevant difference in the parse total (~800μs)

### Tree-sitter: Full Parse (with pool, via native)
- Go (~200 LOC): ~855 microseconds, 5.34 megabytes per second
- Python (~200 LOC): ~995 microseconds, 5.53 megabytes per second
- JavaScript (~200 LOC): ~872 microseconds, 5.03 megabytes per second

ANTLR: In-process (without sidecar)
- PL/SQL (~100 LOC): ~1.05 milliseconds, 2.49 megabytes per second
- PostgreSQL (~80 LOC): ~829 microseconds, 2.82 megabytes per second

Launcher Size

- Core Binary: 239MB (all grammars compiled)
- Launcher: **3.2MB** (no embedded grammars)
