---
title: Otimizar Performance do Sync IDE Adapter e IDE Rule
status: done
created: 2026-06-18
updated: 2026-06-18
tags: [performance, sync, ide-adapter, idempotency]
---

# Otimizar Performance do Sync: IDE Adapter e IDE Rule

## Objective

O `graphit sync` (e `graphit_sync` MCP) estava lento mesmo quando nada mudava no projeto.
The steps "Updating IDE rules" and "Syncing IDE adapter" performed unnecessary I/O operations (read + write)
Each invocation, regardless of any actual changes made.

## Implementation Details

Problem 1 — INLINE_0 was performing legacy cleanup before immutability.

**Arquivo**: `internal/hub/adapters/ide/mandate.go`

Before: The function always executed `cleanupLegacy(targetPath)` before checking if the content was present.
The trigger was already correct. The `cleanupLegacy` reads and potentially overwrites the AGENTS.md file three times.
(Bloqueos de Legacy), and it was called five times by Sync (once per module: Knowledge, AST, Hub, Memory)
improvements).

**Depois**:
Read the file only once at the beginning.
2. Verify if legacy blocks are present (string search without destructive input/output)
3. Verify if the content of the trigger is already correct
4. If there is no legacy content and the data is correct, return nil (no writes).
Only executes `cleanupLegacy` if necessary

Added Functions:
- `readMandateContentFromString(content string) string` — extrai o inner do bloco mandato a partir
"Reading from an already processed string, avoiding disk re-reading"

Fixed in Legacy Detection Format: The detection format was incorrect (_`START`/_`END` in the name), corrected.
para o formato real do `HTMLBlockStyle`: `<!-- MARKER -->` / `<!-- END MARKER -->`.

### Problema 2 — `copyArtifact` folder-mode sempre fazia RemoveAll+copyDirAll

**Arquivo**: `internal/hub/adapters/ide/base.go`

Before: For artifacts of type INLINE_0 (folder mode), the code always did:
```go
The code snippet provided translates to:

```
_ = dest.RemoveAll();  // Destroys the destination
```
return copyDirAll(...)   // recopia tudo
```
The `copyDirAll` uses `copyFile` with idempotence (compares size + content), but as the `RemoveAll`
It destroyed destiny before, idempotence never worked.

After: Before `RemoveAll`, calls `dirContentsEqual(src, dst)`. If directories are
Identical (same tree, same sizes, same content), returns immediately in its default state.

Function Added
Brazilian Portuguese:
- `dirContentsEqual(src, dst string) bool` — compares two directories using `filepath.Walk`:
Verifies that all files from the src directory exist in the dst directory with the same size and content, and that the dst directory exists.
There are no extra files. Use `filepath.SkipAll` for short-circuiting in case of divergence.

## Files Changed

| File | Change | Reason |
|---|---|---|
Inline 0 | Modified | Ensuring idempotence before cleanup of legacy; helper reads Mandate content from string.
Here is the translation:

"_`internal/hub/adapters/ide/base.go`_ | Modified | dirContentsEqual for folder mode, skip RemoveAll+Copy when identical"

## Key Decisions

Reading of the file once instead of reading it multiple times (one for cleaning up legacy, etc.)
A book for reading the mandate), now we read it once and pass the content as a string to the helpers.
Using ``strings.Contains`` for string search detection is O(n) but only once, which is efficient.
  vs. compilar regex e executar `RemoveBlockStyled` 3 vezes (que inclui leitura+escrita).
Verification of directory equality involves two walks.
Add-ons just for counting files. This could be combined, but readability is better this way.
The cost is minimal compared to RemoveAll + copy.

## Notes

The warning "INLINE_0" in the build is from a third-party package.
Irrelevant and pre-existing.
Added in Go 1.20, it is available in the version used by the project.
The tests for package `internal/hub/adapters/ide` pass without modification.
