---
title: "Auto-safe TOON for heavy columns"
status: done
created: 2026-06-20
updated: 2026-06-20
tags: [ast, toon, mcp, ai_optimized, bug-fix]
---

# Auto-safe TOON for heavy columns in FormatRecordsTOON

## Problem

`graphit_ast_query` com `ai_optimized: true` (o default) chamava `FormatRecordsTOON`
que passava **todos** os campos para o formato pipe-delimited TOON. Isso quebrava o output
quando a query retornava colunas "pesadas" — como `file.source`, `docstring`, ou qualquer
valor com `\n` ou `|`:

- Newlines viravam separadores de linha extras → formato TOON corrompido
- Pipes no código viravam separadores de coluna → colunas ilegíveis

O comportamento era silencioso: a saída era sintaticamente válida para o parser MCP,
mas semanticamente incorreta para a IA.

## Solução

Adicionada **detecção automática** em `FormatRecordsTOON` (`internal/ast/toon.go`):

1. **Primeiro passo** (scan): varre todos os registros para detectar quais colunas
   contêm strings com `\n` ou `|` — chamadas de "colunas pesadas".

2. **Renderização em dois níveis**:
   - Colunas leves: emitidas normalmente na tabela pipe-delimited.
   - Colunas pesadas: emitidas como blocos nomeados **fora** da tabela, no formato:
     ```
     --- <colname> ---
     <conteúdo completo preservado>
     ```
     Para múltiplos registros:
     ```
     --- <colname>[0] ---
     <conteúdo do registro 0>
     --- <colname>[1] ---
     <conteúdo do registro 1>
     ```

3. O conteúdo **não é truncado nem sanitizado** — preservado integralmente.

## Exemplo

Query: `MATCH (fn:Function {name: 'Foo'})<-[:CONTAINS]-(f:File) RETURN fn.name, fn.line_number, f.source`

**Antes (quebrado):**
```
results[3]{f.source|fn.line_number|fn.name}:
  func Foo() {
  return 42   ← linha extra corrompendo o TOON
}|10|Foo
```

**Depois (correto):**
```
results[3]{fn.line_number|fn.name}:
  10|Foo

--- f.source ---
func Foo() {
	return 42
}
```

## Não requer opt-out

O comportamento é completamente automático. A flag `ai_optimized` não precisa ser
alterada — o sistema detecta e adapta o formato internamente baseado no conteúdo
dos dados.

## Arquivos alterados

| Arquivo | Mudança |
|---|---|
| `internal/ast/toon.go` | `isHeavyString()` helper + lógica de two-pass em `FormatRecordsTOON` |
| `internal/ast/toon_test.go` | 4 novos testes para heavy columns |

## Verificação

- `go test ./internal/ast/ -run TestFormatRecordsTOON -v` — 10/10 PASS
- `go build ./...` — build limpo
