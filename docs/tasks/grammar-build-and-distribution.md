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
| 1. Projeto | `.graphit/ast/queries/` | `.graphit/grammars/treesitter/` | `.graphit/grammars/antlr/` |
| 2. Global | `~/.graphit/ast/queries/` | `~/.graphit/grammars/treesitter/` | `~/.graphit/grammars/antlr/` |
| 3. Runtime | `~/.graphit/runtime/<v>/ast/queries/` | `~/.graphit/runtime/<v>/grammars/treesitter/` | `~/.graphit/runtime/<v>/grammars/antlr/` |

---

## Makefile Targets

### `make grammars`

Compila todas as 42 gramáticas (37 tree-sitter + 5 ANTLR).

### `make grammars-treesitter`

Compila as 37 gramáticas tree-sitter como shared libraries (`.so`).

Saída: `.build/grammars/treesitter/tree-sitter-<lang>.so`

As gramáticas estão organizadas em 3 categorias:

| Categoria | Fonte | Variáveis |
|:---|:---|:---|
| **A: smacker** | `go-tree-sitter` module (27 langs) | `TS_GRAMMARS_SMACKER_*` |
| **B: External** | Go modules separados (6 langs) | `TS_GRAMMARS_EXTERNAL` |
| **C: Local** | `internal/ast/treesitter/<lang>/` (4 langs) | `TS_GRAMMARS_LOCAL` |

### `make grammars-antlr`

Compila os 5 sidecars ANTLR como binários Go standalone.

Saída: `.build/grammars/antlr/antlr-sidecar-<grammar>`

Cada sidecar inclui apenas UMA gramática, selecionada por build tag (`grammar_<name>`).

### `make grammars-clean`

Remove `.build/grammars/`.

---

## Hub Distribution

O artifact type `language` no Hub distribui **YAML + .grammar** juntos.

### Estrutura de um language artifact no Hub

```
artifact/languages/<project-id>/<lang-name>/<version>/
  plsql.yaml              # Definição de queries
  antlr-plsql.grammar     # Fat archive com binários cross-platform
```

### Fluxo de Install

```bash
graphit hub install plsql-lang
```

1. Hub baixa o artifact (YAML + .grammar)
2. `plsql.yaml` → `<project>/.graphit/ast/queries/plsql.yaml`
3. `antlr-plsql.grammar` → extrai binário da plataforma atual → `<project>/.graphit/grammars/antlr/antlr-sidecar-plsql`
4. DynGrammarLoader/SidecarDriver encontram diretamente — sem cache

### Fluxo de Uninstall

```bash
graphit hub uninstall plsql-lang
```

1. Remove `plsql.yaml` de `<project>/.graphit/ast/queries/`
2. Remove `antlr-sidecar-plsql` de `<project>/.graphit/grammars/antlr/`

### Código

- Install: `internal/hub/service.go` — `case TypeLanguage`
- Extração: `internal/hub/grammar_install.go` — `installGrammarArchive()`
- Uninstall: `internal/hub/service.go` — `preUninstallHook()` + `uninstallGrammarFiles()`

---

## Launcher (Defaults)

O launcher embute apenas **~16 gramáticas tree-sitter** mais comuns como defaults. ANTLR não é default — instala via Hub.

### Defaults embutidos

Definidos em `DEFAULT_TS_GRAMMARS` no Makefile:

```
golang python javascript typescript tsx java kotlin
rust csharp cpp c ruby php swift dart sql
```

### Como funciona

1. `make build-linux` chama `$(call bundle_grammars)` que copia os `.so` defaults de `.build/grammars/treesitter/` para `cmd/launcher/runtime/grammars/treesitter/`
2. Go `embed.FS` inclui tudo em `cmd/launcher/runtime/`
3. Na primeira execução, `extractRuntime()` extrai para `~/.graphit/runtime/<version>/grammars/treesitter/`
4. `DynGrammarLoader.searchDirs()` encontra os `.so` no runtime dir

---

## .grammar Archive Format (GRMT v1)

Fat archive binário cross-platform com compressão zstd.

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

### Código

- Read/Write: `internal/ast/grammar_archive.go`
- Hub extraction: `internal/hub/grammar_install.go`

---

## Runtime Loading

### Tree-sitter: `DynGrammarLoader`

Usa CGO `dlopen`/`dlsym` para carregar `.so` dinâmicos.

```go
loader := ast.NewDynGrammarLoader()
lang, err := loader.Load("go")  // busca tree-sitter-golang.so nos search dirs
parser.SetLanguage(lang)
```

- Busca candidatos: `tree-sitter-<lang>-<os>-<arch>.so`, `tree-sitter-<lang>-<os>.so`, `tree-sitter-<lang>.so`
- Cache em `sync.Map` — zero allocations após primeiro load
- Código: `internal/ast/treesitter_dynload.go`

### ANTLR: `SidecarDriver`

Usa subprocessos (IPC via stdin/stdout com protocol buffers).

```go
driver := ast.NewSidecarDriver("antlr-sidecar-plsql")
tree, err := driver.Parse(source)
```

- Pool de processos reutilizáveis
- Isolamento de memória (gramáticas ANTLR são pesadas)
- Código: `internal/ast/antlr_sidecar.go`, `internal/ast/antlr_adapter.go`

---

## Performance

### Tree-sitter (CGO dlopen vs Native)

| Métrica | Native CGO | dlopen SharedLib | Overhead |
|:---|---:|---:|:---|
| Parse ~1KB Go | 26,387 ns | 30,223 ns | +14% |
| Grammar lookup | 46 ns | 14 ns | 3x mais rápido |
| Allocations | 6 allocs | 6 allocs | Idêntico |

### ANTLR (Sidecar pooled vs In-process)

| Métrica | In-process | Sidecar pooled | Melhoria |
|:---|---:|---:|:---|
| Parse PL/SQL | 1.44ms | 232μs | 6.2x mais rápido |
| Allocations | 54 allocs | 6 allocs | 8.9x menos |

---

## Como Adicionar uma Nova Gramática

### Tree-sitter — smacker/go-tree-sitter

Se a gramática já está no módulo `smacker/go-tree-sitter`:

1. Adicionar à variável correta no `Makefile`:

```makefile
# Sem scanner ou scanner.c simples:
TS_GRAMMARS_SMACKER_SIMPLE := ... nova_lang

# Com alloc.c (scanner usa ../alloc.h):
TS_GRAMMARS_SMACKER_ALLOC := ... nova_lang

# Com scanner.cc (C++):
TS_GRAMMARS_SMACKER_CXX := ... nova_lang
```

2. Criar `internal/ast/queries/nova_lang.yaml` com queries Tree-sitter S-expression
3. Rodar: `make grammars-treesitter`

### Tree-sitter — External Go module

Se a gramática está em um Go module separado:

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

3. Se tem subdir (ex: `xml/src/`):
```makefile
    newlang:github.com/org/tree-sitter-newlang@v1.0.0/sublang
```

4. Criar o YAML e rodar: `make grammars-treesitter`

### Tree-sitter — Local vendored

Se a gramática tem código C local:

1. Criar `internal/ast/treesitter/nova_lang/` com `parser.c` (e opcionalmente `scanner.c`)
2. Adicionar ao `Makefile`:
```makefile
TS_GRAMMARS_LOCAL := ... nova_lang
```
3. Criar o YAML e rodar: `make grammars-treesitter`

### ANTLR v4

1. Gerar parser Go com ANTLR e colocar em `internal/ast/antlr/nova_lang/`
2. Criar driver com build tag em `cmd/graphit-antlr-sidecar/driver_nova_lang.go`:
```go
//go:build grammar_nova_lang
package main
// ... register parser
```
3. Adicionar ao `Makefile`:
```makefile
ANTLR_GRAMMARS := ... nova_lang
```
4. Criar `internal/ast/queries/nova_lang.yaml` com `parser: antlr4`
5. Rodar: `make grammars-antlr`

### Tornar Default no Launcher

Adicionar à lista `DEFAULT_TS_GRAMMARS` no `Makefile`:

```makefile
DEFAULT_TS_GRAMMARS := ... nova_lang
```

### Publicar no Hub

Incluir o `.grammar` archive junto com o YAML no artifact `language`:

```bash
graphit hub submit nova-lang --type language
```

O diretório do artifact deve conter:
- `nova_lang.yaml`
- `tree-sitter-nova_lang.grammar` (ou `antlr-nova_lang.grammar`)
