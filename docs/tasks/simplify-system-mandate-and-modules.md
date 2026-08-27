Task: Simplify System Mandate and Module Rules

**Data:** 2026-07-15
**Escopo:** `internal/hub/adapters/ide/mandate.go`, `internal/memory/rule.go`, `internal/ast/rule.go`, `internal/hub/rule.go`, `internal/knowledge/rule.go`, `internal/improvements/rule.go`, `internal/mcpstdio/server.go`, `internal/mcpstdio/tools_test.go`

## Problema

The mandate of the system (`AGENTS.md`) and the rules of the modules had long, redundant instructions. Moreover, the model was obliged to emit a block `<graphit>MEM:0|AST:0...</graphit>` and each MCP output included a lengthy and redundant reminder block `_SYS_REMINDER`.

Solution

1. **Simplification of Mandate Preamble:** Updated the preamble of the mandate in `internal/hub/adapters/ide/mandate.go` to remove the requirement for block `<graphit>` and dump model, replacing it with a direct instruction to read and use the corresponding skill in `.agents/skills/`.
2. **Removal of _SYS_REMINDER:** Modified `SysReminder` to an empty string and modified `server.go` to not include the block if it is empty.
3. **Simplification of MandateTriggers:** Each module (`ast`, `memory`, `hub`, `knowledge`, `improvements`) had its function `MandateTrigger()` simplified to only return the instruction for reading the corresponding skill.
4. **Removal of Unused Variables:** Cleaned up all string and auxiliary function declarations that are not used in functions `MandateTrigger()`.
5. **Update of Tests:** Adjusted `TestJsonResult` in `tools_test.go` to not require block `_SYS_REMINDER` if `ide.SysReminder` is empty.

## Resultado

The file `AGENTS.md` is now concise, containing only instructions for reading the respective skills.
The model no longer needs to emit debug blocks `<graphit>` and the tool responses MCP do not contain any footnotes `_SYS_REMINDER`.
Unit tests compile and pass successfully.

## Arquivos Modificados

- `internal/hub/adapters/ide/mandate.go`
- `internal/memory/rule.go`
- `internal/ast/rule.go`
- `internal/hub/rule.go`
- `internal/knowledge/rule.go`
- `internal/improvements/rule.go`
- `internal/mcpstdio/server.go`
- `internal/mcpstdio/tools_test.go`
- `AGENTS.md`
