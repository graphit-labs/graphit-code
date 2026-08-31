---
title: Content-named entities stop embedding their own text in their uid
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [ast, uid, correctness, comments]
---

# Content-named entities stop embedding their own text in their uid

## Objective

The engineer showed a real UID from indexing the Linux kernel:

```
"UID":"arch/alpha/boot/bootp.c::arch/alpha/boot/bootp.c\nCopyright (C) 1997 Jay Estabrook\n..."
```

— a Comment node whose entire multi-line license header is embedded in its own
identifier — and asked whether a SHA256 of the content would be better: smaller on
average, and free of the special-character problems raw text carries.

## Reasoning

`entityUID(relFilePath, name, ctxName string) string` (`writer.go:115`) is
`relFilePath + "::" + name` for every entity, with no exception. For a `Comment` (or
`Value`, `AttributeValue`, `Text` — the labels this codebase's own
`contentNamedLabels` map already documents as "whose `name` IS their content rather
than an identifier someone chose"), `name` is not a chosen identifier at all — it is
the literal comment/string/attribute text, unbounded in size and free to contain
anything a source file can contain: newlines, quotes, null bytes.

Confirmed the bug reproduces on THIS repository's own store before touching
anything: the ten largest `entity.UID` values were all `Comment` nodes, up to
1.2 KB, some with embedded newlines (`GRAPHIT_SHARD_FOOTPRINT=... go test -run
TestBiggestUIDs`, throwaway probe, not committed).

**Is a hash better than the current scheme? For content-named entities specifically,
yes — but a hash of CONTENT ALONE is the wrong hash, and would trade a visible bug
for a silent one.** A UID collision here does not "share a key" harmlessly: the
`entityRows[[2]string{uid, label}]` dedup in `ConvertToCache` treats matching
`(uid, label)` as "the same entity seen twice" and merges the second occurrence into
the first instead of keeping both. A content hash makes exactly this happen for two
genuinely different comments that happen to say the same thing — a repeated license
header, a copy-pasted `// TODO`, the same empty string literal appearing twice in
one file. Traced this and found it was **already happening**, at an even earlier
stage: `extractCommentsTS` (tree-sitter) and `extractCommentsAntlr` deduplicate
extracted comments with `seen[text]`, so a repeated comment was never even reaching
`ConvertToCache` as more than one entity — its `REFERENCES` edge to what it
documents silently vanished along with it. This predates today's session; the UID
question is what surfaced it.

The fix therefore does not use a hash at all. It gives each content-named entity a
disambiguator that is **unique per real occurrence by construction**, not by the
astronomically-low-but-nonzero-collision-probability a hash offers:

- `Value` / `AttributeValue` / `Text`: the entity's own index within its data key's
  slice (`contentNamedUID`, `dataKey + "#" + index`). Both the pre-pass and the main
  pass in `ConvertToCache` iterate `pf.Entities[dataKey]` in the same order, so the
  same index always names the same occurrence. Cheaper than hashing (an integer
  increment, no hash function), and zero collision risk — an index can never repeat
  within its own slice by construction, where a hash's collision probability is only
  ever small, never zero.
- `Comment`: needs a stronger guarantee, because a Comment's `REFERENCES` edge
  (`comment → what it documents`) is built in a SEPARATE pass
  (`extractCommentsTS` / `extractCommentsAntlr`) and a separate slice
  (`pf.References`, which other extraction passes also append to — its index does
  not correlate with `pf.Entities["comments"]`'s). `commentUIDName(line)` — the
  comment's own start line — is what both sides already have independently
  (`Entity.Line` and `ReferenceInfo.Line` are set from the same `line` variable at
  extraction time) without needing to see each other's data.

## Implementation

- `internal/ast/cache_convert.go`: added `contentNamedUID` (index-based) and
  `commentUIDName` (line-based). The main entity loop now special-cases
  `label == LabelComment` → `commentUIDName`, other `contentNamedLabels` →
  `contentNamedUID`. The pre-pass that populates `nameToUID` (for context/parent
  lookups) now skips content-named entities entirely — nothing legitimately looks
  one up by name, since nothing nests inside a Value/Text/Comment node.
- `internal/ast/treesitter_adapter.go` (`extractCommentsTS`): the `seen` dedup is
  now keyed by the comment node's byte position (`node.StartByte()`), not its text —
  this still guards against the same node matching twice under an alternation
  query, without conflating two different comments with identical text.
  `ReferenceInfo.SourceName` is now `commentUIDName(line)` instead of the raw
  comment text, so it resolves (via the existing generic
  `entityUID(relPath, ref.SourceName, "")` fallback in `ConvertToCache`'s reference
  loop — unchanged) to exactly the uid the Comment entity itself gets. No other code
  needed to change to make the two agree.
- `internal/ast/antlr_adapter.go` (`extractCommentsAntlr`): identical fix — `seen`
  keyed by `c.Start` (`[2]int`, line+column) instead of text, `SourceName` set to
  `commentUIDName(c.Start[0])`.

## Test Cases & Acceptance Criteria

```gherkin
Given a file with two comments carrying byte-for-byte identical text
When it is converted to cache (tree-sitter) or run through extractCommentsAntlr
Then both survive as distinct Comment entities with distinct uids
  And each has its own REFERENCES edge, not one shared or one dropped

Given a comment much longer than a typical identifier
When its entity is built
Then the uid stays short (bounded, not proportional to the comment's length)

Given the same (path, dataKey, index) or (path, line) input
When contentNamedUID / commentUIDName is called twice
Then it returns the identical value both times (deterministic)
```

- `TestContentNamedEntitiesGetDistinctUIDsEvenWithIdenticalText`,
  `TestContentNamedEntityUIDDoesNotScaleWithContentLength`,
  `TestContentNamedUIDIsStableAcrossRuns` (new, `content_named_uid_test.go`).
- `TestRepeatedIdenticalCommentsAreBothIndexedAntlr` (new, `comment_entity_test.go`)
  — same proof via the ANTLR path.
- `commentsOf` (existing test helper in `comment_entity_test.go`, used by
  `TestCommentsAreEntitiesInEveryLanguage` / `TestCommentsAreEntitiesInAntlrLanguages`)
  updated to match a comment to its reference by LINE rather than by assuming
  `ReferenceInfo.SourceName` equals the comment's own text — the assumption this
  task's fix deliberately breaks. Both pre-existing tests still pass with the fix.
- `go test ./internal/ast/... -timeout 600s`: green.
- `go test -race -count=1 ./internal/ast/ -timeout 600s`: green — caught and fixed a
  real (pre-existing, test-infrastructure-only) race along the way: two of this
  task's new tests both called `stageGrammar(t, "go", ...)` under `t.Parallel()`,
  which races on a shared extension-table map the helper was never built to have
  two parallel callers mutate concurrently. Fixed by not marking those two tests
  parallel — the helper itself is unrelated pre-existing test fragility, out of
  scope here.
- `go vet ./internal/ast/`: clean.
- Re-measured with a throwaway probe that reparses ~400 of this repo's own `.go`
  files through the full pipeline: the ten largest `entity.UID` values are now
  ordinary long identifiers (ANTLR-generated parser constant names, ~117-121 bytes)
  — no Comment node appears in the top 10 anymore, where before the fix all ten were
  Comments up to 1.2 KB.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/cache_convert.go` | Modified | `contentNamedUID`, `commentUIDName`; main loop special-cases content-named labels; pre-pass skips them |
| `internal/ast/treesitter_adapter.go` | Modified | `extractCommentsTS`'s dedup keyed by position not text; `SourceName` uses `commentUIDName` |
| `internal/ast/antlr_adapter.go` | Modified | `extractCommentsAntlr`, same fix |
| `internal/ast/content_named_uid_test.go` | Created | new tests, listed above |
| `internal/ast/comment_entity_test.go` | Modified | `commentsOf` helper fixed to match by line; new ANTLR duplicate-comment test |

## What this does NOT change

- `Value.UID` / `AttributeValue.UID` / `Text.UID` are index-based, not
  content-addressed — two DIFFERENT files' identical string literals get different
  uids (as they always did; `entityUID` always prefixes the path), and now two
  occurrences WITHIN one file do too (previously they could collide when both had
  empty `Name`, e.g. two `""` literals — this is fixed as a side effect, not
  separately tested).
- Real declared entities (Function, Class, …) are untouched — their `name` is a
  chosen identifier, not their own content, and stays exactly what it was.
- The searchable text of a comment is unaffected — `Entity.Name` (what full-text
  search and display read) still carries the actual comment text; only the
  identifier built from it changed.

## Progress Log

### 2026-08-31
Confirmed the reported UID shape reproduces on this repo's own store (biggest
`entity.UID`s are all Comment nodes, up to 1.2 KB). Rejected a straight content
hash: traced how a UID collision is actually handled (`ConvertToCache`'s
`(uid, label)` dedup merges, doesn't error) and found the SAME failure mode already
live one layer up — `extractCommentsTS`/`extractCommentsAntlr` deduplicate by
comment TEXT, silently dropping every repeated comment (and its REFERENCES edge)
before `ConvertToCache` ever saw more than one. Fixed both extraction sites to key
on position instead, and gave content-named entities an occurrence-unique
(not content-derived) uid: index-based for Value/AttributeValue/Text, line-based
for Comment specifically (to keep its uid computable independently on both the
entity side and the separately-built REFERENCES-edge side, which a content hash or
an index could not do without one side seeing the other's data).

Hit and fixed a real data race along the way, in test infrastructure only: two new
parallel tests staging the same grammar concurrently raced on a shared map in
`stageGrammar`. Fixed by not running those two tests in parallel with each other.

Re-measured against ~400 of this repo's own files reparsed fresh: no Comment node
in the ten largest `entity.UID`s anymore; the largest are now ordinary
ANTLR-generated identifier names around 120 bytes.
