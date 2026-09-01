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

| Priority | YAML Queries | TS Grammars | ANTLR Grammars |
|:---|:---|:---|:---|
| 1. Project | `.graphit/ast/queries/` | `.graphit/grammars/treesitter/` | `.graphit/grammars/antlr/` |
| 2. Global | `~/.graphit/ast/queries/` | `~/.graphit/grammars/treesitter/` | `~/.graphit/grammars/antlr/` |
| 3. Runtime | `~/.graphit/runtime/<v>/ast/queries/` | `~/.graphit/runtime/<v>/grammars/treesitter/` | `~/.graphit/runtime/<v>/grammars/antlr/` |

---

## Makefile Targets

### `make grammars`

Compiles all 42 grammars (37 tree-sitter + 5 ANTLR).

### `make grammars-treesitter`

Compiles the 37 tree-sitter grammars as shared libraries (`.so`).

Output: `.build/grammars/treesitter/tree-sitter-<lang>.so`

The grammars are organized in 3 categories:

| Category | Source | Variables |
|:---|:---|:---|
| **A: smacker** | `go-tree-sitter` module (27 langs) | `TS_GRAMMARS_SMACKER_*` |
| **B: External** | Separate Go modules (6 langs) | `TS_GRAMMARS_EXTERNAL` |
| **C: Local** | `internal/ast/treesitter/<lang>/` (4 langs) | `TS_GRAMMARS_LOCAL` |

### `make grammars-antlr`

Compiles the 5 ANTLR sidecars as standalone Go binaries.

Output: `.build/grammars/antlr/antlr-sidecar-<grammar>`

Each sidecar includes only ONE grammar, selected by build tag (`grammar_<name>`).

### `make grammars-clean`

Removes `.build/grammars/`.

---

## Hub Distribution

The `language` artifact type in the Hub distributes **YAML + .grammar** together.

### Structure of a language artifact in the Hub

```
artifact/languages/<project-id>/<lang-name>/<version>/
  plsql.yaml              # Definição de queries
  antlr-plsql.grammar     # Fat archive com binários cross-platform
```

### Install Flow

```bash
graphit hub install plsql-lang
```

1. The Hub downloads the artifact (YAML + .grammar)
2. `plsql.yaml` → `<project>/.graphit/ast/queries/plsql.yaml`
3. `antlr-plsql.grammar` → extracts the binary for the current platform → `<project>/.graphit/grammars/antlr/antlr-sidecar-plsql`
4. DynGrammarLoader/SidecarDriver find it directly — no cache

### Uninstall Flow

```bash
graphit hub uninstall plsql-lang
```

1. Removes `plsql.yaml` from `<project>/.graphit/ast/queries/`
2. Removes `antlr-sidecar-plsql` from `<project>/.graphit/grammars/antlr/`

### Code

- Install: `internal/hub/service.go` — `case TypeLanguage`
- Extraction: `internal/hub/grammar_install.go` — `installGrammarArchive()`
- Uninstall: `internal/hub/service.go` — `preUninstallHook()` + `uninstallGrammarFiles()`

---

## Launcher (Defaults)

The launcher embeds only the **~16 most common tree-sitter grammars** as defaults. ANTLR is not a default — it is installed via the Hub.

### Embedded defaults

Defined in `DEFAULT_TS_GRAMMARS` in the Makefile:

```
golang python javascript typescript tsx java kotlin
rust csharp cpp c ruby php swift dart sql
```

### How it works

1. `make build-linux` calls `$(call bundle_grammars)`, which copies the default `.so` files from `.build/grammars/treesitter/` to `cmd/launcher/runtime/grammars/treesitter/`
2. Go's `embed.FS` includes everything under `cmd/launcher/runtime/`
3. On the first run, `extractRuntime()` extracts to `~/.graphit/runtime/<version>/grammars/treesitter/`
4. `DynGrammarLoader.searchDirs()` finds the `.so` files in the runtime dir

---

## .grammar Archive Format (GRMT v1)

Cross-platform binary fat archive with zstd compression.

```
Header (16 bytes):
  Magic:    "GRMT" (4 bytes)
  Version:  uint32 LE (1)
  Count:    uint32 LE (número de plataformas)
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

### Code

- Read/Write: `internal/ast/grammar_archive.go`
- Hub extraction: `internal/hub/grammar_install.go`

---

## Runtime Loading

### Tree-sitter: `DynGrammarLoader`

Uses CGO `dlopen`/`dlsym` to load dynamic `.so` files.

```go
loader := ast.NewDynGrammarLoader()
lang, err := loader.Load("go")  // busca tree-sitter-golang.so nos search dirs
parser.SetLanguage(lang)
```

- Candidate lookup: `tree-sitter-<lang>-<os>-<arch>.so`, `tree-sitter-<lang>-<os>.so`, `tree-sitter-<lang>.so`
- Cached in a `sync.Map` — zero allocations after the first load
- Code: `internal/ast/treesitter_dynload.go`

### ANTLR: `SidecarDriver`

Uses subprocesses (IPC via stdin/stdout with protocol buffers).

```go
driver := ast.NewSidecarDriver("antlr-sidecar-plsql")
tree, err := driver.Parse(source)
```

- Pool of reusable processes
- Memory isolation (ANTLR grammars are heavy)
- Code: `internal/ast/antlr_sidecar.go`, `internal/ast/antlr_adapter.go`

---

## Performance

### Tree-sitter (CGO dlopen vs Native)

| Metric | Native CGO | dlopen SharedLib | Overhead |
|:---|---:|---:|:---|
| Parse ~1KB Go | 26,387 ns | 30,223 ns | +14% |
| Grammar lookup | 46 ns | 14 ns | 3x faster |
| Allocations | 6 allocs | 6 allocs | Identical |

### ANTLR (Sidecar pooled vs In-process)

| Metric | In-process | Sidecar pooled | Improvement |
|:---|---:|---:|:---|
| Parse PL/SQL | 1.44ms | 232μs | 6.2x faster |
| Allocations | 54 allocs | 6 allocs | 8.9x fewer |

---

## How to Add a New Grammar

### Tree-sitter — smacker/go-tree-sitter

If the grammar is already in the `smacker/go-tree-sitter` module:

1. Add it to the correct variable in the `Makefile`:

```makefile
# Sem scanner ou scanner.c simples:
TS_GRAMMARS_SMACKER_SIMPLE := ... nova_lang

# Com alloc.c (scanner usa ../alloc.h):
TS_GRAMMARS_SMACKER_ALLOC := ... nova_lang

# Com scanner.cc (C++):
TS_GRAMMARS_SMACKER_CXX := ... nova_lang
```

2. Create `internal/ast/queries/nova_lang.yaml` with Tree-sitter S-expression queries
3. Run: `make grammars-treesitter`

### Tree-sitter — External Go module

If the grammar is in a separate Go module:

1. Add it to `go.mod`:
```bash
go get github.com/tree-sitter/tree-sitter-newlang@v1.0.0
```

2. Add it to the `Makefile`:
```makefile
TS_GRAMMARS_EXTERNAL := \
    ... \
    newlang:github.com/tree-sitter/tree-sitter-newlang@v1.0.0
```

3. If it has a subdir (e.g. `xml/src/`):
```makefile
    newlang:github.com/org/tree-sitter-newlang@v1.0.0/sublang
```

4. Create the YAML and run: `make grammars-treesitter`

### Tree-sitter — Local vendored

If the grammar has local C code:

1. Create `internal/ast/treesitter/nova_lang/` with `parser.c` (and optionally `scanner.c`)
2. Add it to the `Makefile`:
```makefile
TS_GRAMMARS_LOCAL := ... nova_lang
```
3. Create the YAML and run: `make grammars-treesitter`

### ANTLR v4

1. Generate the Go parser with ANTLR and place it in `internal/ast/antlr/nova_lang/`
2. Create the driver with a build tag in `cmd/graphit-antlr-sidecar/driver_nova_lang.go`:
```go
//go:build grammar_nova_lang
package main
// ... register parser
```
3. Add it to the `Makefile`:
```makefile
ANTLR_GRAMMARS := ... nova_lang
```
4. Create `internal/ast/queries/nova_lang.yaml` with `parser: antlr4`
5. Run: `make grammars-antlr`

### Making It a Default in the Launcher

Add it to the `DEFAULT_TS_GRAMMARS` list in the `Makefile`:

```makefile
DEFAULT_TS_GRAMMARS := ... nova_lang
```

### Publishing on the Hub

Include the `.grammar` archive together with the YAML in the `language` artifact:

```bash
graphit hub submit nova-lang --type language
```

The artifact directory must contain:
- `nova_lang.yaml`
- `tree-sitter-nova_lang.grammar` (or `antlr-nova_lang.grammar`)
