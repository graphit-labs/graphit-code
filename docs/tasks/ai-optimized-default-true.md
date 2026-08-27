---
title: ai_optimized default true — Remove mandatory flag instruction
status: done
created: 2026-06-18
updated: 2026-06-18
tags: [mcp, ai_optimized, refactor, breaking-change]
---

# ai_optimized default true

## Objective

Remove the mandatory instruction that agents must pass `ai_optimized: true` in every MCP call, making the value `true` the implicit default for both the stdio MCP server and the CLI. Agents that want verbose JSON output must explicitly pass `ai_optimized: false` (opt-out instead of opt-in).

## Implementation Details

### MCP Stdio Tools (opt-in → opt-out)

All input structs in the files `internal/mcpstdio/tools_*.go` that had:

```go
AiOptimized bool `json:"ai_optimized,omitempty" jsonschema:"MANDATORY for AI agents. Set to true..."`
```

They were changed to:

```go
AiOptimized *bool `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
```

The use of `*bool` allows distinguishing between "not passed" (nil → true by default)
and "explicitly passed as false". The helper `aiOpt()` in `server.go` handles
this logic:

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

### CLI (false → true by default)

The `--ai-optimized` flags in the CLI commands were changed from `false` to `true` as the default value:

- `cmd/graphit/commands/ast.go`
- `cmd/graphit/commands/wiki.go`

### Removal of automatic instruction in skills/rules

- `internal/brand/brand.go`: removed function `UniversalAIOptimizedNote()`
- `internal/ast/rule.go`: removed note injection into the rule content
- `internal/hub/adapters/ide/mandate.go`: removida nota do preamble
- `internal/hub/rule.go`: removed note injection
- `internal/knowledge/rule.go`: removed note injection
- `internal/memory/rule.go`: removed note injection
- `internal/improvements/rules.go`: removed note injection

## Key Decisions

- **`*bool` vs custom type**: Chosen `*bool` as the type for `AiOptimized` instead of creating a dedicated type. Simple and straightforward for the opt-out use case.
- **JSON schema `omitempty`**: Kept `omitempty` so that the field does not appear in generated schemas when not provided — reduces noise in MCP schemas.
- **Backward compatibility**: Agents still passing `ai_optimized: true` continue to function normally. The breaking change only affects those who intentionally passed `ai_optimized: false` (unlikely, as it was opt-in).

## Trade-offs & Decisions

- Switch opt-in to opt-out: simplifies usage for agents (who were the default case) and keeps the verbose JSON option for debugging via `false`.
- Removing the notes from skills reduces unnecessarily consumed tokens during each skill read.

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
- Added helper `aiOpt()` in `server.go`
- Refactored all 9 files from `tools_*.go` to `*bool`
- Changed CLI flags in `ast.go` and `wiki.go`
- Removed `UniversalAIOptimizedNote()` from `brand.go`
- Removed all injections in the 5 rules files
- Build validated successfully (`go build ./...`)
