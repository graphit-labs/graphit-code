---
title: "Auto-safe TOON for heavy columns"
status: done
created: 2026-06-20
updated: 2026-06-20
tags: [ast, toon, mcp, ai_optimized, bug-fix]
---

# Auto-safe TOON for heavy columns in FormatRecordsTOON

## Problem

`graphit_ast_query` with `ai_optimized: true` (the default) called `FormatRecordsTOON`
that passed **all** the fields to the pipe-delimited TOON format. This would break the output
when the query returned "heavy" columns — such as `file.source`, `docstring`, or any
value with `\n` or `|`:

- Newlines became extra line separators in corrupted TOON → FORMAT
- Pipes in the code became column separators illegible → columns

The behavior was silent: the output was syntactically valid for the MCP parser,
but semantically incorrect for AI.

Solution 

Added ** auto detect ** in `FormatRecordsTOON` (`internal/ast/toon.go`):

1. **First step** (scan): scans all records to detect which columns
   contain strings with `\n` or `|` — called "heavy columns".

2. ** Two-level rendering **:
   - Light columns: normally issued in the pipe-delimited table.
   - Heavy columns: issued as blocks named * * outside * * of the table, in the format:
     ```
     --- <colname> ---
The content is preserved as originally stated.
     ```
For multiple records:
     ```
     --- <colname>[0] ---
Content of Registration 0
     --- <colname>[1] ---
Content of Registration 1
     ```

3. The content ** is neither truncated nor sanitized** — fully preserved.

## Exemplo

Query: `MATCH (fn:Function {name: 'Foo'})<-[:CONTAINS]-(f:File) RETURN fn.name, fn.line_number, f.source`

**Before (broken):**
```
results[3]{f.source|fn.line_number|fn.name}:
  func Foo() {
  return 42   ← linha extra corrompendo o TOON
}|10|Foo
```

**After (correct):**
```
results[3]{fn.line_number|fn.name}:
  10|Foo

--- f.source ---
func Foo() {
	return 42
}
```

## Does not require opt-out

The behavior is completely automatic. The `ai_optimized` flag does not need to be
changed — the system detects and adapts the format internally based on the content
of the data.

## Arquivos alterados

| File | Change |
|---|---|
| `internal/ast/toon.go` | `isHeavyString()` helper + two-pass logic in `FormatRecordsTOON` |
| `internal/ast/toon_test.go` | 4 new tests for heavy columns |

verification

- `go test ./internal/ast/ -run TestFormatRecordsTOON -v` — 10/10 PASS
- `go build ./...` — build limpo
