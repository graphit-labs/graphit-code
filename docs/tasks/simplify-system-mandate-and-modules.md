# task: simplificar mandato do sistema e regras dos módulos

**Data:** 2026-07-15
**Escopo:** `internal/hub/adapters/ide/mandate.go`, `internal/memory/rule.go`, `internal/ast/rule.go`, `internal/hub/rule.go`, `internal/knowledge/rule.go`, `internal/improvements/rule.go`, `internal/mcpstdio/server.go`, `internal/mcpstdio/tools_test.go`

## Problema

O mandato do sistema (`AGENTS.md`) e as regras dos módulos possuíam instruções longas e redundantes. Além disso, o modelo era obrigado a emitir um bloco `<graphit>MEM:0|AST:0...</graphit>` e a saída de cada ferramenta MCP incluía um bloco de lembrete `_SYS_REMINDER` volumoso e redundante.

## Solução

1. **Simplificação de Mandate Preamble:** Atualizado o preâmbulo do mandato em `internal/hub/adapters/ide/mandate.go` para remover a obrigatoriedade do bloco `<graphit>` e do dump do modelo, substituindo por uma instrução direta de ler e usar a skill correspondente em `.agents/skills/`.
2. **Remoção de _SYS_REMINDER:** Alterado `SysReminder` para string vazia e modificado `server.go` para não incluir o bloco se estiver vazio.
3. **Simplificação de MandateTriggers:** Cada um dos módulos (`ast`, `memory`, `hub`, `knowledge`, `improvements`) teve sua função `MandateTrigger()` simplificada para apenas retornar a instrução de leitura da skill correspondente.
4. **Remoção de Variáveis Não Usadas:** Limpas todas as declarações de strings e funções auxiliares sem uso nas funções `MandateTrigger()`.
5. **Atualização de Testes:** Ajustado `TestJsonResult` em `tools_test.go` para não exigir o bloco `_SYS_REMINDER` se `ide.SysReminder` estiver vazio.

## Resultado

- O arquivo `AGENTS.md` agora é conciso, contendo apenas instruções para ler as respectivas skills.
- O modelo não precisa mais emitir blocos de depuração `<graphit>` e as respostas de ferramentas MCP não contêm mais o rodapé `_SYS_REMINDER`.
- Os testes unitários compilam e passam com sucesso.

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
