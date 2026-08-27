# Grammar Build & Distribution System

## Overview

Graphit uses two parsing backends — **Tree-sitter** and **ANTLR v4** — to support 42 programming languages. This document covers:

1. How grammars are **compiled** (Makefile targets)
2. How grammars are **distributed** (Hub + Launcher)
3. How grammars are **loaded** at runtime (DynGrammarLoader / SidecarDriver)
4. How to **add a new grammar**

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                        Build (Makefile)                            │
│  make grammars-treesitter → .build/grammars/treesitter/*.so        │
│  make grammars-antlr      → .build/grammars/antlr/antlr-sidecar-* │
└────────────┬───────────────────────────────────┬───────────────────┘
             │                                   │
     ┌───────▼───────┐                  ┌────────▼────────┐
     │   Launcher    │                  │      Hub        │
     │ (embed ~16 TS │                  │ (distribui YAML │
     │  defaults)    │                  │  + .grammar)    │
     └───────┬───────┘                  └────────┬────────┘
             │                                   │
             │  extract                          │  hub install
             ▼                                   ▼
     ~/.graphit/runtime/<v>/         <project>/.graphit/
       grammars/treesitter/*.so        grammars/treesitter/*.so
                                       grammars/antlr/antlr-sidecar-*
                                       ast/queries/*.yaml
```

## Search Path Hierarchy

Both YAML query files and grammar binaries use the same priority:

Portuguese:
| Priority | YAML Queries | TS Grammars | ANTLR Grammars |
| --- | :--- | :--- | :--- |
| 1. Project | `.graphit/ast/queries/` | `.graphit/grammars/treesitter/` | `.graphit/grammars/antlr/` |
| 2. Global | `~/.graphit/ast/queries/` | `~/.graphit/grammars/treesitter/` | `~/.graphit/grammars/antlr/` |
| 3. Runtime | `~/.graphit/runtime/<v>/ast/queries/` | `~/.graphit/runtime/<v>/grammars/treesitter/` | `~/.graphit/runtime/<v>/grammars/antlr/` |

English:

---

## Makefile Targets

### `make grammars`

Compile all 42 grammars (37 Tree-Sitter + 5 ANTLR).

### `make grammars-treesitter`

Compile the 37 grammars into shared libraries (__INLINE_11__).

Output: __INLINE__ 12___

The grammars are organized into three categories:

Portuguese:
| Category | Source | Variables |
| :--- | :--- | :--- |
| **A: smacker** | Inline 13 module (27 languages) | Inline 14 |
| **B: External** | Go modules separated (6 languages) | Inline 15 |
| **C: Local** | `internal/ast/treesitter/<lang>/` (4 languages) | `TS_GRAMMARS_LOCAL` |

English:
A: smacker
- Inline 13 module (27 languages)
- Inline 14

B: External
- Go modules separated (6 languages)

C: Local
- `internal/ast/treesitter/<lang>/` (4 languages)
- `TS_GRAMMARS_LOCAL`

### `make grammars-antlr`

Compile the five Sidecars of ANTLR into standalone Go binaries.

Output: _INLINE_ 19___

Each sidecar includes only ONE grammar, selected by build tag (__INLINE_20__).

### `make grammars-clean`

Remove `.build/grammars/`.

---

## Hub Distribution

O artifact type `language` no Hub distribui **YAML + .grammar** juntos.

Structure of a Language Artifact in a Hub

```
artifact/languages/<project-id>/<lang-name>/<version>/
PlSQL.yaml # Definition of Queries
ANTLR-PL/SQL. Grammar - Fat Archive with Cross-Platform Binaries
```

Flow of Installation

```bash
graphit hub install plsql-lang
```

1. Hub downloads the artifact (YAML + .grammar)
2. Inline 24 → Inline 25
3. Inline 26 → extracts binary from the current platform → Inline 27
4. DynGrammarLoader/SidecarDriver find directly — without cache

Uninstallation Flow

```bash
graphit hub uninstall plsql-lang
```

Remove 28 from INLINE_29
Remove 30 from INLINE_31

Code

Install: __inline 32__ — __inline 33__
Extraction: __inline 34__ — __inline 35__
Uninstallation: __inline 36__ — __inline 37__ + __inline 38__

---

## Launcher (Defaults)

The launcher embeds only **about 16 grammatical Tree-Sitter rules by default**. ANTLR is not the default; it must be installed through the Hub.

### Defaults embutidos

Defined in `DEFAULT_TS_GRAMMARS` of the Makefile:

```
golang python javascript typescript tsx java kotlin
rust csharp cpp c ruby php swift dart sql
```

How it works

1. The `make build-linux` INLINE_41___ class calls the `.so` DEFAULTS of the `.build/grammars/treesitter/` class to the `cmd/launcher/runtime/grammars/treesitter/` class.
2. Go `embed.FS` includes everything in `cmd/launcher/runtime/`.
3. In the first execution, `extractRuntime()` extracts to `~/.graphit/runtime/<version>/grammars/treesitter/`.
4. `DynGrammarLoader.searchDirs()` finds the `.so` at runtime dir

---

## .grammar Archive Format (GRMT v1)

Binary fat archive with platform-independent compression using ZSTD.

```
Header (16 bytes):
  Magic:    "GRMT" (4 bytes)
  Version:  uint32 LE (1)
Count:   uint32 LE (number of platforms)
  Reserved: uint32 LE

Entry Table (120 bytes × count):
  OS:             char[16]    ("linux", "darwin", "windows")
  Arch:           char[16]    ("amd64", "arm64")
  SymbolName:     char[64]    ("tree_sitter_go")
  DataOffset:     uint64 LE
  CompressedSize: uint64 LE
  OriginalSize:   uint64 LE

Data Section:
  [zstd-compressed binary data for each platform]
```

Code

- Read/Write: `internal/ast/grammar_archive.go`
- Hub extraction: `internal/hub/grammar_install.go`

---

## Runtime Loading

### Tree-sitter: `DynGrammarLoader`

Use CGO _INLINE_54/_INLINE_55 to load _INLINE_56 dynamically.

```go
loader := ast.NewDynGrammarLoader()
lang, err := loader.Load("go")  // busca tree-sitter-golang.so nos search dirs
parser.SetLanguage(lang)
```

Search for candidates: `tree-sitter-<lang>-<os>-<arch>.so`, `tree-sitter-<lang>-<os>.so`, `tree-sitter-<lang>.so`
Cache in `sync.Map` — no allocations after the first load
Code: `internal/ast/treesitter_dynload.go`

### ANTLR: `SidecarDriver`

Use subprocesses (via IPC through stdin/stdout with Protocol Buffers).

```go
driver := ast.NewSidecarDriver("antlr-sidecar-plsql")
tree, err := driver.Parse(source)
```

Pool of reusable processes
Isolation of memory (ANTLR grammars are heavy)
Code: `internal/ast/antlr_sidecar.go`, `internal/ast/antlr_adapter.go`

---

## Performance

### Tree-sitter (CGO dlopen vs Native)

Parse ~1KB Go: 26,387 ns; Grammar lookup: 46 ns; Overhead: +14%.

Grammar lookup: 14 ns; Allocations: 6 allocs; Identical.

### ANTLR (Sidecar pooled vs In-process)

Parse PL/SQL: 1.44 milliseconds, 232 microseconds faster.
Allocations: 54 allocations, 6 fewer allocations.

---

How to Add a New Grammar

### Tree-sitter — smacker/go-tree-sitter

If grammar is already in module `smacker/go-tree-sitter`:

Add to the correct variable in __INLINE__ 66:

```makefile
# Sem scanner ou scanner.c simples:
TS_GRAMMARS_SMACKER_SIMPLE := ... nova_lang

# Com alloc.c (scanner usa ../alloc.h):
TS_GRAMMARS_SMACKER_ALLOC := ... nova_lang

# Com scanner.cc (C++):
TS_GRAMMARS_SMACKER_CXX := ... nova_lang
```

2. Create inline 67 with Tree-sitter S-expression queries.
3. Run: inline 68

### Tree-sitter — External Go module

If the grammar is in its own Go module:

1. Adicionar ao `go.mod`:
```bash
go get github.com/tree-sitter/tree-sitter-newlang@v1.0.0
```

2. Adicionar ao `Makefile`:
```makefile
TS_GRAMMARS_EXTERNAL := \
    ... \
    newlang:github.com/tree-sitter/tree-sitter-newlang@v1.0.0
```

3. If there is a subdirectory (e.g., _INLINE_71__):
```makefile
    newlang:github.com/org/tree-sitter-newlang@v1.0.0/sublang
```

4. Criar o YAML e rodar: `make grammars-treesitter`

### Tree-sitter — Local vendored

If grammar has local C code:

Create ___INLINE_73__ with ___INLINE_74__ (optionally ___INLINE_75__)
```makefile
TS_GRAMMARS_LOCAL := ... nova_lang
```
3. Criar o YAML e rodar: `make grammars-treesitter`

### ANTLR v4

1. Generate a Go parser with ANTLR and place it in `internal/ast/antlr/nova_lang/`.
2. Create a driver with a build tag in `cmd/graphit-antlr-sidecar/driver_nova_lang.go`:
```go
//go:build grammar_nova_lang
package main
// ... register parser
```
3. Adicionar ao `Makefile`:
```makefile
ANTLR_GRAMMARS := ... nova_lang
```
4. Create `internal/ast/queries/nova_lang.yaml` with `parser: antlr4`
5. Run: `make grammars-antlr`

### Tornar Default no Launcher

Add to list `DEFAULT_TS_GRAMMARS` in `Makefile`:

```makefile
DEFAULT_TS_GRAMMARS := ... nova_lang
```

### Publicar no Hub

Include the `.grammar` archive along with the YAML in the artifact `language`:

```bash
graphit hub submit nova-lang --type language
```

The artifact directory should contain:
- `nova_lang.yaml`
- `tree-sitter-nova_lang.grammar` (or `antlr-nova_lang.grammar`)
