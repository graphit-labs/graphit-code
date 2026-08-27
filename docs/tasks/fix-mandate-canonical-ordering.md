# fix: mandate canonical module ordering

**Data:** 2026-06-20
**Escopo:** `internal/hub/adapters/ide/mandate.go`, `internal/hub/adapters/ide/ide_test.go`

## Problema

The inline 2 removed and re-added the existing trigger to the end of the mandate block. This caused modules added for the first time (e.g., inline 3 via inline 4) to change position, generating large diffs even without content changes.

Root Cause

```go
// antes — sempre appenda ao final
inner = inner + "\n" + wrapped
```

Solution

Replaced the append strategy with an ordered complete rewrite:

1. **Extracts all existing triggers from a `map[string]string`**
2. Update the target trigger in the map
3. **Rewrites the inner with `canonicalTriggerOrder`:**
   `mem_rule → ast_rule → hub_rule → doc_rule → imp_rule`
Triggers that are unknown (installed on the hub) come after the canonical ones, sorted alphabetically.

## Resultado

- The inline 10th project is already synchronized → diff zero
- With the new module → the module appears in canonical position, not at the end
- The order of calls to `InstallRule` no longer matters

## Arquivos Modificados

- `internal/hub/adapters/ide/mandate.go`: adicionado `canonicalTriggerOrder`, `parseTriggers`, `assembleTriggers`; `UpsertMandateTrigger` reescrito
- `internal/hub/adapters/ide/ide_test.go`: 4 novos testes (`TestParseTriggers`, `TestAssembleTriggers_CanonicalOrder`, `TestAssembleTriggers_UnknownTagsSorted`, `TestUpsertMandateTrigger_CanonicalOrdering`, `TestUpsertMandateTrigger_Idempotent`)
