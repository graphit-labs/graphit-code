# Security Fix: Critical Path Traversal in Context Handlers

**Status:** Completed
**Date:** 2026-05-31
**Severity:** CRITICAL (C-1, C-2)

## Summary

Fixed path traversal vulnerabilities in the MCP stdio tools where user-controlled
`input.Context` values were passed directly to `filepath.Join` and `os.RemoveAll`
without sanitization, allowing an attacker to delete arbitrary directories.

## Vulnerabilities Fixed

### C-1 — Path Traversal in `memory_remove` (tools_memory.go)

- **Location:** `tools_memory.go`, `memory_remove` handler
- **Risk:** `input.Context` was used unsanitized in `filepath.Join(projectDir, brand.DotDir(), "memory", input.Context)` followed by `os.RemoveAll`. A malicious context name like `../../..` could delete arbitrary directories.
- **Fix:** Added `sanitizeContextName(input.Context)` call before the `filepath.Join`.

### C-2 — Path Traversal in `knowledge_remove` (tools_knowledge.go)

- **Location:** `tools_knowledge.go`, `knowledge_remove` handler
- **Risk:** Identical pattern to C-1 but targeting the `knowledge` subdirectory.
- **Fix:** Added `sanitizeContextName(input.Context)` call before the `filepath.Join`.

### Hardening — `memory_sync` (tools_memory.go)

- **Location:** `tools_memory.go`, `memory_sync` handler
- **Risk:** `input.Context` passed directly to `memory.NewMemoryServiceForContext()`. While not a direct `os.RemoveAll` call, the context name flows into file paths downstream.
- **Fix:** Added `sanitizeContextName(input.Context)` call before use.

### Hardening — `knowledge_sync` (tools_knowledge.go)

- **Location:** `tools_knowledge.go`, `knowledge_sync` handler
- **Risk:** `input.Context` used in branch names (`fmt.Sprintf("knowledge/project/%s", input.Context)`) and passed to `knowledge.EnsureContextCopy()` and `knowledge.WikiDirForContext()`.
- **Fix:** Added `sanitizeContextName(input.Context)` call before all uses.

## New Function: `sanitizeContextName` (context.go)

A shared validation function added to `internal/mcpstdio/context.go`:

```go
func sanitizeContextName(name string) (string, error)
```

**Validation rules:**
1. Rejects empty strings
2. Applies `filepath.Base()` to strip directory components
3. Rejects `.`, `..`, and path separator characters
4. Rejects names containing `/` or `\` (defense-in-depth after `filepath.Base`)

## Files Changed

| File | Change |
|---|---|
| `internal/mcpstdio/context.go` | Added `sanitizeContextName()` function, added `"strings"` import |
| `internal/mcpstdio/tools_memory.go` | Applied sanitization in `memory_remove` and `memory_sync` handlers |
| `internal/mcpstdio/tools_knowledge.go` | Applied sanitization in `knowledge_remove` and `knowledge_sync` handlers |

## Testing

Verified via `make ci` — all builds and tests pass.
