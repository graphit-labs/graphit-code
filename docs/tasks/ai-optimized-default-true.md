---
title: ai_optimized default true — Remove mandatory flag instruction
status: done
created: 2026-06-18
updated: 2026-06-18
tags: [mcp, ai_optimized, refactor, breaking-change]
---

# ai_optimized default true

## Objective

Remover a instrução obrigatória de que agentes precisam passar `ai_optimized: true`
em cada chamada MCP, tornando o valor `true` o padrão implícito tanto no servidor
MCP stdio quanto na CLI. Agentes que queiram output JSON verbose devem passar
`ai_optimized: false` explicitamente (opt-out ao invés de opt-in).

## Implementation Details

### MCP Stdio Tools (opt-in → opt-out)

Todos os structs de input nos arquivos `internal/mcpstdio/tools_*.go` que tinham:

```go
AiOptimized bool `json:"ai_optimized,omitempty" jsonschema:"MANDATORY for AI agents. Set to true..."`
```

Foram alterados para:

```go
AiOptimized *bool `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
```

O uso de `*bool` permite distinguir entre "não passado" (nil → true por padrão)
e "passado explicitamente como false". O helper `aiOpt()` em `server.go` trata
essa lógica:

```go
func aiOpt(v *bool) bool {
    if v == nil {
        return true
    }
    return *v
}
```

**Arquivos alterados:**
- `internal/mcpstdio/tools_ast.go`
- `internal/mcpstdio/tools_cluster.go`
- `internal/mcpstdio/tools_daemon.go`
- `internal/mcpstdio/tools_dream.go`
- `internal/mcpstdio/tools_hub.go`
- `internal/mcpstdio/tools_knowledge.go`
- `internal/mcpstdio/tools_lifecycle.go`
- `internal/mcpstdio/tools_memory.go`
- `internal/mcpstdio/tools_wiki.go`
- `internal/mcpstdio/server.go` (helper `aiOpt` adicionado)

### CLI (false → true por padrão)

As flags `--ai-optimized` nos comandos CLI foram alteradas de `false` para `true`
como valor padrão:

- `cmd/graphit/commands/ast.go`
- `cmd/graphit/commands/wiki.go`

### Remoção de instrução automática nos skills/rules

- `internal/brand/brand.go`: removida função `UniversalAIOptimizedNote()`
- `internal/ast/rule.go`: removida injeção da nota no conteúdo do rule
- `internal/hub/adapters/ide/mandate.go`: removida nota do preamble
- `internal/hub/rule.go`: removida injeção da nota
- `internal/knowledge/rule.go`: removida injeção da nota
- `internal/memory/rule.go`: removida injeção da nota
- `internal/improvements/rules.go`: removida injeção da nota

## Key Decisions

- **`*bool` vs custom type**: Escolhido `*bool` como tipo para `AiOptimized` em vez
  de criar um tipo dedicado. Simples e direto para o caso de uso de opt-out.
- **JSON schema `omitempty`**: Mantido `omitempty` para que o campo não apareça
  em schemas gerados quando não passado — reduz ruído nos schemas MCP.
- **Compatibilidade retroativa**: Agentes que ainda passem `ai_optimized: true`
  continuam funcionando normalmente. A quebra de compatibilidade é apenas para
  quem passava `ai_optimized: false` intencionalmente (improvável, era opt-in).

## Trade-offs & Decisions

- Troca opt-in por opt-out: simplifica o uso para agentes (que eram o caso padrão)
  e mantém a opção de JSON verbose para debugging via `false`.
- A remoção da nota dos skills reduz tokens consumidos desnecessariamente a cada
  leitura de skill.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/brand/brand.go` | Removed `UniversalAIOptimizedNote` | No longer needed |
| `internal/mcpstdio/server.go` | Added `aiOpt()` helper | Centralize *bool nil check |
| `internal/mcpstdio/tools_*.go` (9 files) | `bool` → `*bool`, description update, if checks | Default true |
| `cmd/graphit/commands/ast.go` | `--ai-optimized` default false → true | CLI parity |
| `cmd/graphit/commands/wiki.go` | `--ai-optimized` default false → true | CLI parity |
| `internal/ast/rule.go` | Removed note injection | Brand cleanup |
| `internal/hub/adapters/ide/mandate.go` | Removed note from preamble | Brand cleanup |
| `internal/hub/rule.go` | Removed note injection | Brand cleanup |
| `internal/knowledge/rule.go` | Removed note injection | Brand cleanup |
| `internal/memory/rule.go` | Removed note injection | Brand cleanup |
| `internal/improvements/rules.go` | Removed note injection | Brand cleanup |

## Progress Log

### 2026-06-18
- Adicionado helper `aiOpt()` em `server.go`
- Refatorados todos os 9 arquivos `tools_*.go` de `bool` para `*bool`
- Alteradas flags CLI em `ast.go` e `wiki.go`
- Removida `UniversalAIOptimizedNote()` de `brand.go`
- Removidas todas as injeções nos 5 arquivos de rules
- Build validado com sucesso (`go build ./...`)
