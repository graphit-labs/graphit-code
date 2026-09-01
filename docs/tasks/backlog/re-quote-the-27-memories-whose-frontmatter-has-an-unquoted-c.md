# Re-quote the 27 memories whose frontmatter has an unquoted colon, and check internal/knowledge for the same exposure

## Problem

A memory title is a free-text sentence. A sentence containing `: ` is not valid YAML unless
quoted, so a memory titled like

```yaml
title: Telemetria do Hub: eventos vão para refs/events/*, nunca para uma branch
```

has a frontmatter block that does not parse. Until 2026-09-01 `ParseMemoryFrontmatter`
answered that with an empty struct and reported nothing, which made 47 files in this
project's scope silently unclassified AND made every re-render path a data-loss path. See
`docs/tasks/memory-revision-chain-searchable-history.md` and the memory
"A memory title containing ': ' made its frontmatter unparseable".

That is fixed: `ParseMemoryFrontmatterOK` reports the outcome, `quoteUnquotedScalars`
recovers the block, and every write path refuses to render a frontmatter it could not read.

**What remains is the data.** 27 live memories in
`memory-project-01KSH1CRFFG8Z74B5ZS78WW808` still carry the unquoted colon. They are
readable through the recovering parse, and each one heals on its next write because writes
go through `yaml.Marshal` — so this is not urgent. But until they are rewritten, every read
of them pays the recovery pass, and the recovery pass cannot be removed while they exist.

## Work

1. Add a repair pass — beside `RepairForkedMemories` in `internal/memory/repair.go`, called
   from the same place — that finds every memory whose raw frontmatter fails
   `yaml.Unmarshal` but succeeds after `quoteUnquotedScalars`, and rewrites it through
   `renderMemoryFile`. That is a pure re-quote: same fields, same body, correctly escaped.
   It must skip any file that does not parse even after recovery, exactly as
   `backfillChainLinks` does, and it must go through `ScopeStore.WriteFile`.
2. Count how many remain and log it once, so the number is visible rather than inferred. There
   is no memory equivalent of `graphit_knowledge_lint` to report it.
3. Check `internal/knowledge` for the same exposure. It parses its own frontmatter over
   `docs/`, where titles are also free text and the `docs/tasks/` logs routinely have colons
   in their titles. **Not investigated.** If it has the same shape, the fix is the same
   two-part one: report the failure, and never re-render from a failed parse.
4. Once the store is clean and knowledge is checked, decide whether `quoteUnquotedScalars`
   can be deleted or should stay as a permanent tolerance for files arriving from other
   machines.

## Verification

- a script counting unparseable frontmatter blocks in the raw store returns 0 for live
  memories and for archived revisions
- `go test -tags fts5,lancedb ./internal/memory/...` stays green, including
  `TestFrontmatterWithAnUnquotedColonStillParses` and
  `TestAnUnreadableFrontmatterIsNeverRewrittenEmpty`
- `graphit_memory_important` lists every memory that declares `important: true` in the raw
  store — which is the read that was most visibly wrong, since an unparseable block hid the
  flag entirely
