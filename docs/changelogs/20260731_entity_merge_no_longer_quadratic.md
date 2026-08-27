# Entity merge stops scanning the whole slice

**Date:** 2026-07-31
**Scope:** `internal/ast/parser.go`, `internal/ast/entity_merge_test.go`
**Origin:** one worker sat for minutes on a file while the others had already left

---

## What happened

`AddOrMergeEntity` scanned every already-registered entity under the same data key on each
insertion, looking for one with the same identity to complete:

```go
for i := range pf.Entities[dataKey] {
    existing := &pf.Entities[dataKey][i]
    if existing.GraphLabel != e.GraphLabel || existing.Name != e.Name ||
        existing.Context != e.Context || existing.Line != e.Line {
        continue
    }
    ...
}
```

O(n²) per file, with n = entities on that key.

## Why only now

While an entity was a declaration, n stayed in the low thousands and cost didn't show. After
`c72a8338`/`ff924816` — key and value became nodes, declared value becomes a node — n started
tracking the number of tokens in the file.

Aggravating: all value nodes carry the **same** `GraphLabel`, so the cheap discriminator at the start of the comparison discards nothing and it always falls through to comparing literal text.

## The fix

A `map[entityIdentity]int` index — identity is (label, name, context, line) — kept alongside
the slice, with O(n) rebuild when length diverges and identity verification on hit.

Rebuild on divergence is necessary because this file is not the sole writer: adapters
take pointers into the slice and rewrite fields in later passes, and callers
build `ParsedFile` with `Entities` already populated. Mismatched length means
rebuild instead of trusting.

`AddEntity` also feeds the index — without it, a later `AddOrMergeEntity` wouldn't see
what it inserted and would add a duplicate.

The index lives in an unexported field, so it doesn't reach the shard cache.

## Measurements

100k merges: **71.8 ms**.

The other front's dedupe tests remain green (`TestCreateTableYieldsTableEntityWithItsColumns`,
`TestOracleSchemaGraphIsQueryable`), plus 5 new cases.

## Important caveat

This **was not** the cause of the stall in `private-corpus`, though it was diagnosed
as such. The fix was already installed when the stall was reproduced, and the profile
showed time in `resolveParentContextTS`, upstream of `AddOrMergeEntity` — parsing never
got to produce entities to merge. See
`20260802_entities_know_their_parent_and_ascent_is_memoized.md`.

The quadratic scan was real and worth fixing on its own.

## Known risk

Three passes mutate entities in place after parsing — `treesitter_adapter.go:883` and `:997`,
`antlr_adapter.go:679`, `helper.go:84`. All write only to `Properties`, never to identity
fields, so the index doesn't go stale. A future pass that mutates `Name`, `Context`,
`Line` or `GraphLabel` in place would break this premise; the hit is verified by identity,
so degradation would be a duplicate, not corruption.
