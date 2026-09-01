# fix: mandate canonical module ordering

**Date:** 2026-06-20
**Scope:** `internal/hub/adapters/ide/mandate.go`, `internal/hub/adapters/ide/ide_test.go`

## Problem

`UpsertMandateTrigger` removed the existing trigger and re-appended it at the end of the mandate block. That made modules added for the first time (e.g. `imp_rule` via `graphit sync`) change position, producing large diffs even with no change in content.

## Root Cause

```go
// antes — sempre appenda ao final
inner = inner + "\n" + wrapped
```

## Solution

The append strategy was replaced by a complete ordered rewrite:

1. `parseTriggers(inner)` — extracts every existing trigger into a `map[string]string`
2. Updates the target trigger in the map
3. `assembleTriggers(triggers)` — rewrites the inner following `canonicalTriggerOrder`:
   `mem_rule → ast_rule → hub_rule → doc_rule → imp_rule`
   Unknown triggers (hub-installed) come after the canonical ones, sorted alphabetically

## Result

- `graphit sync` on an already synced project → zero diff
- `graphit sync` with a new module → the module appears in the canonical position, not at the end
- The call order of `InstallRule` no longer matters

## Modified Files

- `internal/hub/adapters/ide/mandate.go`: added `canonicalTriggerOrder`, `parseTriggers`, `assembleTriggers`; `UpsertMandateTrigger` rewritten
- `internal/hub/adapters/ide/ide_test.go`: 4 new tests (`TestParseTriggers`, `TestAssembleTriggers_CanonicalOrder`, `TestAssembleTriggers_UnknownTagsSorted`, `TestUpsertMandateTrigger_CanonicalOrdering`, `TestUpsertMandateTrigger_Idempotent`)
