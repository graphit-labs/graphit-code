---
title: Memory revision chain becomes two-way and searchable, and the forked _important_ twins are repaired
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [memory, wiki, search, history, repair]
---

# Memory revision chain becomes two-way and searchable, and the forked `_important_` twins are repaired

## Objective

Two things, and the second one is why the first is safe to do.

**1. The revision history of a memory becomes a first-class, searchable, readable page.**
Today a superseded revision is archived to `history/<id>/%04d.md` and made invisible on
purpose: every listing in `internal/memory` does a one-level `os.ReadDir` and skips
directories, so an archived revision is never indexed, never compiled, never returned by
search. The requested model inverts that decision while keeping its benefit:

- history is compiled into the memory wiki and retrievable with `graphit_wiki_source`,
  exactly like any live memory
- the archive file name becomes a ULID instead of a zero-padded counter — ULIDs are
  lexicographically time-ordered and collision-free, so the ordering property of the
  counter survives without the counter
- every superseded revision carries `next` in addition to `previous`, so the chain is
  walkable in both directions
- when a superseded revision matches a search, the result carries it **plus** the id of
  the chain's current head, and the agent decides which one to read
- when two hits in the same result set belong to the same chain, **only the head is
  returned, with no reference to the superseded ones**. The dedup happens as early as
  the pipeline allows, which is before any result reaches the agent
- the memory rule documents all of it, including that `previous` is how you go back in
  time

**2. The 184 forked `<id>_important_.md` twins in this project's memory scope are
repaired, and the fork is made impossible.**
Measured on 2026-09-01 in scope `memory-project-01KSH1CRFFG8Z74B5ZS78WW808`: 496
top-level `.md` files for 312 distinct memory ids. Every one of the 184
`<id>_important_.md` files has a matching `<id>.md`, both satisfy
`isMemorySourceFile`, so both compile — the wiki holds 498 pages, with `<slug>.md`
and `<slug>_2.md` side by side, and every `graphit_memory_search` in this project has been
splitting BM25 relevance across two copies of the same memory.

## Reasoning

### Why the two halves belong in one task

The repair needs somewhere to put content that exists nowhere else, and the searchable
chain is that place. Without it the only options for a divergent twin are "delete and
lose it" or "leave it and keep the duplicate". With it, a divergent twin becomes what it
actually is: a superseded revision of the chain, archived, searchable, and annotated with
the current head.

### Root cause of the fork — it is not a legacy layout

The first reading was "the old layout encoded importance in the file name and the
migration never cleaned up". That is wrong, and the evidence is in the phantom's own
frontmatter:

```yaml
id: 01M0CWRQ6VBRTPQBTK6T4ZNW3G_important_
previous: history/01M0CWRQ6VBRTPQBTK6T4ZNW3G_important_/0001.md
revision: 2
tags: [memory, project]
```

The **id itself carries the suffix**, it has its own `history/<id>_important_/`
directory, and it reached revision 2. So a write path recovered the id from the FILE NAME
— `MemoryIDFromFileName` strips only `.md`, so `<id>_important_.md` yields
`<id>_important_` — and then wrote a *new* memory under that corrupted id. The memory was
not duplicated by a stale file; it was **forked into a twin with a corrupted id, which
then evolved independently**. Note also that the twin's `tags` are `[memory, project]`:
it lost `type` and `important` on the way, which is the signature of a write path that
rebuilt frontmatter instead of preserving it.

`MemoryIDFromFileName` has nine callers. The ones that matter are the ones whose result
reaches a write: `loadMemorySnapshots` in `consolidate.go` feeds ids into
`UpdateMemoryTyped`, and an update writes `MemoryFileName(id)`. That closes the loop and
makes it self-perpetuating: as long as a `<id>_important_.md` exists, a consolidation pass
can update it and recreate both the file and its history directory.

So the guard is not "clean the files". It is **the frontmatter's `id` is authoritative and
a file name is only a fallback**, plus a name whitelist so a non-ULID stem can never be
compiled as a memory again.

### What the reconciliation actually costs — measured, not assumed

| pairs | finding | resolution |
|---|---|---|
| 179 of 184 | body byte-identical to the live memory | delete the twin, no information exists only there |
| 4 of 184 | body differs from live, but is byte-identical to an existing `history/<id>/0001.md` | delete the twin, the content is already archived |
| 1 of 184 | `01M0CWRQ6VBRTPQBTK6T4ZNW3G` — body matches nothing | archive it into the chain as a superseded revision, then delete the twin |

Plus 4 `history/<id>_important_/` directories, every file of which is byte-identical to a
file already in the corresponding `history/<id>/` — safe to remove.

So the destructive part of the repair touches content that provably exists elsewhere, with
exactly one exception, and that exception is preserved rather than deleted.

### Why the repair is code and not a script

`hub.bucket` is set globally (`graphit`), so these objects are in the shared bucket. Two
consequences decide the shape of the fix:

- a local `rm` does not hold. `ScopeStore.Pull` MERGES and never deletes locally to match
  the remote, so the objects come back on the next sync. Removal has to go through
  `ScopeStore.RemoveFile`, which records the deletion for `Publish`.
- every other clone of this bucket has the same 184 twins. A script on this machine fixes
  one checkout. An idempotent repair invoked from `IndexMemories` fixes all of them, once
  each, without anyone having to know it was needed.

### Alternatives considered and dropped

| Option | Why not |
|---|---|
| Keep history invisible and just fix the duplicates | Refuses the requested feature, and leaves the divergent twin with nowhere to go but deletion |
| Put the chain metadata in the wiki DB schema (`WikiChunk`, `BM25Result`) so dedup is a SQL filter | Correct in the long run and cheaper per query, but it changes a schema shared with the knowledge wiki and both BM25 paths (compiled and the .md fallback). The hits are at most `top_k` pages; reading their frontmatter is a handful of small reads. Recorded as debt |
| Rename the existing `%04d.md` archives to ULIDs | Every rename invalidates a `previous` pointer in a live memory. Legacy names stay valid addresses, and they sort before every ULID (`"0001" < "01K…"`), so the ordering property holds in a mixed directory. New archives get ULIDs |
| Make `next` always point at the head instead of the immediate successor | Loses forward stepping, which is half of what was asked for. The head is already recoverable from the archive's `id`, so pointing `next` one step forward is strictly more information for the cost of one extra small write per update |
| Delete the divergent twin because the live memory is "better" | It is a genuinely different rewrite of the same memory, produced by a consolidation pass on the wrong twin. Nothing else holds it |

## Plan & Task Breakdown

- [ ] **T1 — Chain fields on the raw memory frontmatter** — Spec: `internal/memory/memory.go`.
  `MemoryFrontmatter` gains `RevisionID` (`revision_id`) and `Next` (`next`), both
  `omitempty`. A live memory carries neither: it is the head. An archived revision carries
  `revision_id` (its own address, equal to its file stem) and `next` (the path of what
  replaced it — another archive, or `<id>.md` for the head). `id` stays the CHAIN id on an
  archive, which is what lets a search result name the current head without walking
  anything. Constraint: `renderMemoryFile` stays the single place the on-disk shape is
  defined, so add and update cannot disagree.

- [ ] **T2 — ULID history addressing** — Spec: `internal/memory/identity.go`.
  `HistoryPath(id, revisionID string)` replaces `HistoryPath(id string, revision int)`, and
  `NewRevisionID()` returns `ulid.Make().String()`. Done means: new archives are
  `history/<id>/<ulid>.md`, legacy `%04d.md` names still resolve as addresses, and a mixed
  directory still sorts chronologically. Constraint: no rename of existing archives — a
  rename breaks a live memory's `previous`.

- [ ] **T3 — Two-way chain on update and on delete** — Spec: `internal/memory/memory.go`,
  `updateMemory` and `archiveBeforeDelete`. On update: mint a revision id, write the archive
  with `revision_id` and `next: <id>.md`, repoint the previously-newest archive's `next` at
  the new archive, and point the live memory's `previous` at it. On delete: the archive's
  `next` is empty, because nothing replaced it and the head is gone. Constraint: a failure
  to repoint the older archive must not fail the update — the chain degrades to
  head-reachable, which is what it is today.

- [ ] **T4 — History compiles into the memory wiki** — Spec: `internal/memory/wiki.go`.
  Collect `history/<id>/*.md` alongside the top-level memories. Slug is
  `SafeSlug(title) + "--" + revisionID` so a revision page can never collide with its live
  page and the slug is stable across builds. Page frontmatter gains `superseded: true`,
  `current: <chain id>`, `revision`, `previous`, `next`, and a `superseded` tag. `keepFiles`
  must include them or the prune pass deletes them on the next build. Done means:
  `graphit_wiki_source` opens a revision page by slug. Constraint: `ListMemories` and
  `ListImportantMemories` stay live-only — history is not "what this project knows".

- [ ] **T5 — Chain dedup and head annotation at search time** — Spec: new
  `internal/memory/search.go`, wired into `internal/mcpstdio/tools_memory.go` and
  `runMemorySearch`. Over-fetch from `wiki.BM25Search`, read `id` / `superseded` / `current`
  from each hit page's frontmatter, group by chain id, then: if the group contains the head,
  emit only the head and nothing about the archives; if the group is archives only, emit
  them annotated with `current`. Trim to `top_k` AFTER dedup, so a caller asking for ten
  gets ten distinct memories. Constraint: must not change knowledge search behaviour — the
  grouping lives in the memory package, not in `internal/wiki`.

- [ ] **T6 — The id comes from the frontmatter, never from the file name** — Spec:
  `internal/memory/{wiki.go,consolidate.go,memory.go,important.go}`. Every site that calls
  `MemoryIDFromFileName` prefers `ParseMemoryFrontmatter(data).ID` and falls back to the
  name only when the frontmatter has none. `isMemorySourceFile` additionally requires a
  26-character Crockford ULID stem. Done means: a `<id>_important_.md` dropped into the raw
  directory is not compiled and cannot be updated into existence again.

- [ ] **T7 — Idempotent repair of the forked twins** — Spec: new
  `internal/memory/repair.go`, called from `IndexMemories`. For each top-level `.md` whose
  stem is not a valid ULID: resolve the real chain id (strip the suffix / read the
  frontmatter), then delete through `RemoveFile` when the body is byte-identical to the live
  memory or to any archive of that chain, and otherwise archive it into the chain first.
  Merge `history/<phantom>/` into `history/<id>/` under the same rule. Constraint: idempotent
  and silent when there is nothing to do, because it runs on every index; and every deletion
  goes through `RemoveFile` so the S3 object goes with it.

- [ ] **T8 — The memory rule teaches the chain** — Spec: `internal/memory/rule.go`. History
  is searchable; a hit may be a superseded revision and says so, carrying the current id;
  the agent never sees both revisions of one chain in one result set; `previous` walks back
  in time and `next` walks forward; a revision page is read with `wiki_source` like any
  other. Constraint: the existing rule tests in `internal/memory/rule_*_test.go` must still
  pass.

- [ ] **T9 — Documentation** — Spec: this log, plus an ADR in `docs/decisions/` for the
  reversal of "history is deliberately invisible", plus the memory spec under `docs/specs/`.

- [ ] **T10 — Verify** — Spec: `go build ./...`, `go test -tags fts5 ./internal/memory/...
  ./internal/wiki/...`, then run the repair against the real store and confirm: 312
  top-level memories, no `_important_` file, no `_2` slug in the memory wiki, a search for a
  previously duplicated title returns exactly one hit, and a search that matches only an old
  revision returns it annotated with the current id.

## Implementation Details

### The chain (T1–T3)

`MemoryFrontmatter` gained two fields. `Next` is the path of what replaced this revision —
another archive, or `<id>.md` when the successor is the live memory — and `RevisionID` is an
archive's own address inside its chain. A live memory carries neither, and
`MemoryFrontmatter.IsArchivedRevision()` is that check. `ID` stays the CHAIN id on an
archive, which is what lets a search hit on an old revision name the current one.

`HistoryPath(id, revisionID string)` replaced `HistoryPath(id string, revision int)`, so
archives are `history/<id>/<ulid>.md`. `NewRevisionID`, `HistoryDirFor`,
`RevisionIDFromHistoryPath` and `IsMemoryID` are the supporting helpers.

`MemoryService.archiveRevision(scope, id, content, nextPath)` is the single archiving path
for both update and delete. It writes the archive through the pure
`archivedRevisionContent(chainID, content, revisionID, nextPath)` — which imposes the chain
id rather than inheriting it, because the repair path archives content whose declared id is
the corrupted one — then calls `repointArchiveNext` to move the previously-newest archive's
`next` onto the new one. A failure to repoint is logged and swallowed: the chain degrades to
what it was before `next` existed, which is not worth failing a write for. On delete
`nextPath` is empty, and that emptiness is what distinguishes the last state of a deleted
memory from a superseded revision.

### Compiling history (T4)

`historySourceFileNames` is the one deliberate walk into `history/`; every other listing in
the package still reads one level and skips directories, which is what keeps an archive out
of the CATALOGUE. `memorySourceFileNames` appends it so a repointed forward pointer
invalidates the stat pre-check.

The per-file work moved into `buildMemDoc(rawDir, rel, processCache, validPaths)`, keyed on
the path relative to the raw directory so an archive caches under
`history/<id>/<rev>.md`. **Supersession is decided by location, not by frontmatter** —
`isHistorySource(rel) || fm.IsArchivedRevision()` — which is the fix for the failure found
during verification, below.

`memDoc.slugBase()` suffixes an archive's slug with its revision id, so a revision page
never collides with the live page it is the history of and the slug is stable across
rebuilds. `memoryEntityPage(doc)` replaced `memoryEntityPageWithHash(...)` and emits
`superseded`, `current`, `revision_id`, `revision`, `previous`, `next`, a `superseded` tag,
a banner naming the current memory, and no staleness nudge. `memoryIndexPage` filters
superseded docs out of the catalogue.

### Search (T5)

`internal/memory/search.go`. `SearchChains` over-fetches `top_k × 4` from
`wiki.BM25Search`, resolves each hit's chain from its page frontmatter with
`wiki.ReadFrontmatterField`, collapses by chain id, then trims — so `top_k` counts distinct
memories rather than index rows. `collapseChains` keeps the chain at the rank its best hit
earned and lets a live revision replace an archived one already held.
`FormatChainResultsTOON` adds the `superseded`/`current` columns **only** when some hit is
superseded, delegating to `wiki.FormatBM25ResultsTOON` otherwise, so an all-current answer
costs exactly what it did before. Wired into `graphit_memory_search` and `runMemorySearch`.

### Identity (T6)

`MemoryIDFor(content, name)` is the single resolver: the declared id, with the file name as a
fallback only for content that declares none. Routed through it: `buildMemDoc`,
`listImportantInDir`, `loadMemorySnapshots`, `ListMemories`. `isForkedMemoryFileName` rejects
`<ulid><anything>.md`, and `dedupMemoryDocsByID` keeps one document per declared id
preferring the file named after it — a name guard and an id guard, because the name guard
alone assumes a naming convention.

`loadMemoryVectors` now skips history sources. Without that, consolidation would compare
every edited memory against its own past and find a duplicate of itself.

### Repair (T7)

`internal/memory/repair.go`, called from `IndexMemories` before the compile. Per twin:
promote when the chain has no live memory, remove when the body already exists in the live
memory or in any archive, otherwise archive into the chain then remove.
`foldForkedHistoryDirs` merges `history/<corrupted>/` into `history/<chain>/`.
`backfillChainLinks` gives pre-existing archives their `revision_id` and `next`, ordering by
sorted name — correct for both schemes, since `0001` precedes every ULID — and leaving `next`
empty on the last revision of a chain whose memory was deleted. Every removal goes through
`ScopeStore.RemoveFile`.

### The frontmatter parse (found during verification)

`ParseMemoryFrontmatterOK(content) (MemoryFrontmatter, bool)` reports the outcome, and
recovers a block that failed by single-quoting the values of known scalar keys
(`quoteUnquotedScalars`). Every write path checks it: `withImportantFlag` refuses the change,
`repointArchiveNext` and `backfillChainLinks` skip the file, and `updatedMemoryContent`,
`archivedRevisionContent` and `promotedMemoryContent` recover the title from the body's H1
via `firstH1`. See the Progress Log for why this became part of the task.

## Use Cases

### UC-01: An agent updates a memory and the previous revision stays reachable
- **Actor**: agent, through `graphit_memory_update`
- **Preconditions**: the memory exists at revision N
- **Main Flow**:
  1. `MemoryService.updateMemory` mints `revisionID` via `NewRevisionID()`
  2. the current file content is written to `history/<id>/<revisionID>.md` with
     `revision_id: <revisionID>` and `next: <id>.md`
  3. the archive that was previously newest has its `next` repointed at the new archive
  4. the live `<id>.md` is rewritten at revision N+1 with
     `previous: history/<id>/<revisionID>.md`
  5. the wiki build compiles a page for the archive under
     `<slug>--<revisionID>`
- **Alternative Flows**:
  - revision 1 → there is no older archive to repoint, step 3 is skipped
- **Error Scenarios**:
  - repointing the older archive fails → logged, the update proceeds; the chain is still
    head-reachable through each archive's `id`
  - archiving fails → the update aborts before overwriting, as today
- **Postconditions**: the chain is walkable in both directions and every revision is
  searchable
- **Affected Files**: `internal/memory/memory.go`, `internal/memory/identity.go`,
  `internal/memory/wiki.go`

### UC-02: A search matches only a superseded revision
- **Actor**: agent, through `graphit_memory_search`
- **Preconditions**: an archived revision's text matches the query; the live head's does not
- **Main Flow**:
  1. `wiki.BM25Search` returns the archive's page among the hits
  2. the memory search layer reads `superseded` and `current` from the hit page
  3. the result is emitted with `superseded: true` and `current: <chain id>`
- **Postconditions**: the agent can read the old revision, and knows the id of the current
  one without another search
- **Affected Files**: `internal/memory/search.go`, `internal/mcpstdio/tools_memory.go`

### UC-03: A search matches both a superseded revision and its current head
- **Actor**: agent, through `graphit_memory_search`
- **Preconditions**: two or more hits share a chain id
- **Main Flow**:
  1. hits are grouped by chain id
  2. the group contains the head, so only the head is emitted
  3. no reference to the superseded revisions appears in the result
  4. `top_k` is applied after this, so the caller still gets `top_k` distinct memories
- **Postconditions**: one memory never occupies two result slots
- **Affected Files**: `internal/memory/search.go`

### UC-04: A store carrying forked `_important_` twins is repaired on the next index
- **Actor**: the memory index — `graphit_memory_index`, the daemon cycle, or `graphit_sync`
- **Preconditions**: the raw directory holds `<id>_important_.md` beside `<id>.md`
- **Main Flow**:
  1. the repair pass identifies every top-level file whose stem is not a ULID
  2. for each, the chain id is resolved and the body compared with the live memory and with
     every archive of the chain
  3. identical → `RemoveFile`; divergent → archived into the chain, then `RemoveFile`
  4. `history/<phantom>/` is merged into `history/<id>/` and removed
  5. `Publish` propagates the deletions to the bucket
- **Alternative Flows**:
  - nothing to repair → the pass returns silently, costing one directory listing
- **Error Scenarios**:
  - the live memory for a phantom does not exist → the phantom is promoted to the live id
    rather than deleted, because it is then the only copy
- **Postconditions**: one file per memory id, and the wiki page count equals the memory
  count
- **Affected Files**: `internal/memory/repair.go`, `internal/memory/wiki.go`

## Test Cases & Acceptance Criteria

### Feature: Two-way revision chain
Ref: UC-01

#### Scenario: Updating a memory archives the previous revision under a ULID
```gherkin
Given a memory at revision 1 with id "01ABC"
When the agent updates its body
Then a file exists under "history/01ABC/" whose stem is a 26-character ULID
  And that file carries "next: 01ABC.md"
  And that file carries its own "revision_id" equal to its stem
  And the live "01ABC.md" carries "previous" pointing at that file
  And the live "01ABC.md" carries no "next"
```

#### Scenario: A second update repoints the older archive forward
```gherkin
Given a memory at revision 2 with one archived revision
When the agent updates it again
Then the older archive's "next" points at the newer archive
  And the newer archive's "next" points at the live memory
  And walking "previous" from the live memory reaches every revision in reverse order
```

### Feature: Searchable history
Ref: UC-02

#### Scenario: A superseded revision is returned with the current id
```gherkin
Given a memory whose revision 1 mentioned "copy+swap" and whose current revision does not
When the agent searches memory for "copy+swap"
Then the result includes the superseded revision
  And that result is marked as superseded
  And that result names the id of the current revision
```

### Feature: Chain deduplication
Ref: UC-03

#### Scenario: Only the head survives when both match
```gherkin
Given a memory whose current revision and archived revision both match "auth token"
When the agent searches memory for "auth token"
Then exactly one result for that chain is returned
  And it is the current revision
  And no superseded revision of that chain appears in the result
```

#### Scenario Outline: top_k counts distinct memories, not pages
```gherkin
Given "<chains>" distinct memory chains each with "<revisions>" revisions all matching the query
When the agent searches with top_k "<k>"
Then "<returned>" results are returned

Examples:
  | chains | revisions | k  | returned |
  | 3      | 4         | 3  | 3        |
  | 1      | 5         | 5  | 1        |
  | 10     | 2         | 5  | 5        |
```

### Feature: The id is never taken from the file name
Ref: UC-04

#### Scenario: A non-ULID file name is not compiled as a memory
```gherkin
Given a file "01ABC_important_.md" in the raw directory
When the memory wiki is generated
Then no page is written for it
  And the page count equals the number of ULID-named memories plus their archived revisions
```

#### Scenario: An update uses the frontmatter id, not the file name
```gherkin
Given a file "01ABC_suffix.md" whose frontmatter declares "id: 01ABC"
When a consolidation pass updates that memory
Then the write targets "01ABC.md"
  And no file named "01ABC_suffix.md" is created
```

### Feature: Repair of forked twins
Ref: UC-04

#### Scenario: An identical twin is deleted
```gherkin
Given "01ABC.md" and "01ABC_important_.md" with byte-identical bodies
When the repair pass runs
Then "01ABC_important_.md" is removed through RemoveFile
  And "01ABC.md" is untouched
```

#### Scenario: A divergent twin is archived before being deleted
```gherkin
Given "01ABC.md" and "01ABC_important_.md" with different bodies
  And no archive of chain "01ABC" matches the twin's body
When the repair pass runs
Then the twin's content exists under "history/01ABC/" with a ULID stem
  And that archive carries "next: 01ABC.md"
  And "01ABC_important_.md" is removed
```

#### Scenario: The repair pass is idempotent
```gherkin
Given a raw directory with no non-ULID file names
When the repair pass runs twice
Then nothing is written
  And nothing is removed
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/memory/identity.go` | Modified | `HistoryPath` takes a revision id; `NewRevisionID`, `HistoryDirFor`, `RevisionIDFromHistoryPath`, `IsMemoryID`, `MemoryIDFor` |
| `internal/memory/memory.go` | Modified | `Next`/`RevisionID` frontmatter, `archiveRevision`, `repointArchiveNext`, `archivedRevisionContent`, `ParseMemoryFrontmatterOK`, `quoteUnquotedScalars`, `firstH1`, repair call in `IndexMemories` |
| `internal/memory/wiki.go` | Modified | history compiled, `buildMemDoc`, `dedupMemoryDocsByID`, `historySourceFileNames`, `isForkedMemoryFileName`, `memoryEntityPage`, chain page fields |
| `internal/memory/search.go` | Created | `ChainResult`, `SearchChains`, `collapseChains`, `FormatChainResultsTOON` |
| `internal/memory/repair.go` | Created | `RepairForkedMemories`, `foldTwin`, `foldForkedHistoryDirs`, `backfillChainLinks`, `chainIDOf`, `sameMemoryBody` |
| `internal/memory/consolidate.go` | Modified | id from frontmatter, not from the file name — this is the call site that caused the fork |
| `internal/memory/consolidate_similarity.go` | Modified | `isHistorySource`, so a memory is not compared against its own history |
| `internal/memory/important.go` | Modified | id from frontmatter |
| `internal/memory/rule.go` | Modified | the chain section, the raw-layout block, two new triggers |
| `internal/memory/history_test.go` | Modified | ULID addressing, forward-walk test, chain-collapse test, reversed the "not compiled" assertion |
| `internal/memory/repair_test.go` | Created | repair, guards, `top_k`, TOON shape, frontmatter recovery |
| `internal/memory/memory_helpers_test.go` | Modified | `memoryEntityPageWithHash` kept as a test-only adapter for 12 pre-existing call sites |
| `internal/mcpstdio/tools_memory.go` | Modified | `memory_search` goes through `SearchChains` |
| `cmd/graphit/commands/runners.go` | Modified | the CLI marks a superseded hit and names its current memory |
| `docs/decisions/2026-09-01-memory-history-is-searchable-and-the-chain-is-two-way.md` | Created | ADR |
| `docs/specs/memory_module.md` | Modified | "Revision chain" and "Identity integrity" sections |

## Trade-offs & Decisions

- **Chain metadata is read from the hit pages' frontmatter rather than carried in the wiki
  DB.** Cheaper to build and it does not touch a schema shared with the knowledge wiki;
  costs at most `top_k` small file reads per search. Recorded as debt.
- **Legacy `%04d.md` archives are not renamed.** A rename would invalidate the `previous`
  pointer in a live memory. Mixed naming sorts correctly because `"0001"` precedes every
  ULID lexicographically.
- **`next` points at the immediate successor, not at the head.** More information for one
  extra small write per update; the head remains O(1) from the archive's own `id`.
- **`ListMemories` and `ListImportantMemories` stay live-only.** A superseded revision is
  not part of what the project currently knows, and putting it in the catalogue would
  triple the list for no gain. It stays searchable and readable, which is what was asked.

## Technical Debt

- [ ] Chain metadata belongs in the wiki DB (`WikiChunk` / `BM25Result`) so dedup is a
  filter in the query instead of a frontmatter read per hit. Blocked on deciding whether
  supersession is a general wiki concept or a memory-only one.
- [ ] `graphit_memory_list` has no way to ask for a chain's revisions. Reading them today
  means searching or knowing the slug. A `memory_history <id>` surface would close it.
- [ ] The repair pass runs on every index. Once every clone of the bucket has run it, it is
  dead weight and should be removed or gated behind a stamp.
- [ ] **27 live memories in this store still have an unquoted colon in their frontmatter.**
  They are readable again through the recovering parse, and the next write to each rewrites it
  through `yaml.Marshal` correctly — so they heal on touch rather than all at once. A repair
  pass that re-quotes them eagerly would close it, and would also let the recovering parse be
  removed eventually.
- [ ] The recovering parse is line-based and covers the scalar keys in
  `frontmatterScalarKeys`. A legacy block broken some other way — a tab-indented mapping, a
  stray `[` — is still unreadable, and now fails safe instead of silently wiping. Nothing
  reports the count to a user; `graphit_knowledge_lint` has no memory equivalent.
- [ ] `internal/knowledge` parses its own frontmatter and may have the same unquoted-colon
  exposure. Not investigated.

## System Knowledge

- Every listing in `internal/memory` reads exactly one directory level and skips
  subdirectories. That single property is what has been keeping `history/` invisible —
  there is no filter naming it. Changing any of those listings to `WalkDir` turns every
  archived revision into a duplicate memory, which is why T4 adds an explicit, separate
  collection pass instead of widening the existing walk.
- `MemoryIDFromFileName` strips only the `.md` extension. Feeding its result to a write
  path is what forked 184 memories in this project, because the raw directory contained
  file names that were not `<ulid>.md`.
- `ScopeStore.Pull` merges and never deletes locally to match the remote. A local `rm` of a
  memory file is therefore not a deletion — it is undone by the next sync while the object
  is still in the bucket.
- The memory wiki's `_2` slug suffix comes from `wiki.UniqueSlug`, which disambiguates two
  pages that share a title. It is a symptom of duplicate memories, never a cause — and one
  legitimate `_2` remains in this store, for two genuinely different memories with the same
  title.
- **`ParseMemoryFrontmatter` returned a zero struct on a YAML error and reported nothing.**
  Combined with titles being free-text sentences — a title containing `: ` is invalid YAML
  unquoted — this made 47 files in this store silently unclassified, and made every
  re-render path a data-loss path. The lesson generalises past memory: a parser that answers
  a failure with a valid-looking empty value, feeding a writer that trusts it, loses data
  without an error anywhere. `ParseMemoryFrontmatterOK` exists so the failure has to be
  handled.
- Deciding a fact about a file from its frontmatter when the filesystem already encodes it
  is a needless dependency on the file being well-formed. Supersession is now read from the
  path — a file under `history/` is a superseded revision — and that is why legacy archives
  compile correctly without having been migrated first.

## Progress Log

### 2026-09-01

- Investigated the reported duplicate memories. Measured 496 top-level files for 312
  distinct ids in scope `memory-project-01KSH1CRFFG8Z74B5ZS78WW808`, 184 forked twins, 498
  compiled pages.
- Established the root cause: an id recovered from a file name, written back as a new
  memory. Not a stale legacy layout — the twins have corrupted ids, their own history
  directories, and independent revisions.
- Reconciled the twins by content: 179 identical to live, 4 identical to an existing
  archive, 1 divergent with content that exists nowhere else.
- Wrote this log with the plan and the per-task specs.
- T1–T3 landed: the chain fields, ULID addressing, and the two-way archive.
- T4–T5 landed: history compiles into the wiki, and search collapses a chain.
- T6–T7 landed: the id guard and the repair pass. Two bugs surfaced from the repair tests
  and were fixed: `archivedRevisionContent` was carrying the twin's corrupted id into the
  archive, and `foldForkedHistoryDirs` was reporting the source path instead of the new
  archive path.
- T8–T9 landed: the memory rule's chain section and two new triggers, the ADR, the spec.
- **Backed up the raw store before touching it** (`~/.graphit/memory-raw-backup-20260901`),
  then ran the repair. First result: 184 twins gone, 4 forked history directories gone, page
  count exactly `live + archives` — but only 2 of 85 archives compiled as superseded, and 37
  `_2` slugs remained.
- Root cause of that: pre-existing archives declare no `revision_id`, so
  `IsArchivedRevision()` was false and they compiled as if they were live memories — a second
  page under the same title as the memory they are the history of. **Fixed by deciding
  supersession from the file's LOCATION**, which cannot be wrong, and by backfilling
  `revision_id`/`next` onto legacy archives.
- **REGRESSION I INTRODUCED, and the more important finding underneath it.** The backfill
  re-rendered each archive from its parsed frontmatter, and on 20 archives that wrote an
  EMPTY frontmatter over a full one — title, scope, type, tags and timestamps gone, the file
  still valid markdown. Restored all 20 from the backup.
  The cause was not the backfill: `ParseMemoryFrontmatter` returned a zero struct on a YAML
  error and said nothing, and **a title is a free-text sentence, so any title containing
  `: ` was unparseable**. Measured in this store: **27 live memories and 20 archives whose
  frontmatter parsed to nothing**, before any of this work — their type, importance and tags
  invisible to search, listing, `memory_important` and consolidation, and an `update` on any
  of the 27 would have silently wiped them exactly as the backfill did.
  Fixed on both sides: `ParseMemoryFrontmatterOK` reports the outcome and recovers the block
  by quoting scalar values, and every write path now refuses to render a frontmatter it could
  not read. Three regression tests cover it.
- Re-ran the repair on the restored store. Final state verified: 313 live memories, 84
  archived revisions, 0 twins, 0 forked history directories, 0 damaged frontmatter, 397 wiki
  pages (= 313 + 84), 84 superseded pages, 1 remaining `_2` slug which is two genuinely
  different memories sharing a title, and 3 archives with no `next` which are the chains
  whose memory was deleted.
- Verified the two search behaviours against the real store through the MCP tool: a query
  matching an old revision alone returns it with `superseded=yes` and the current memory id;
  a query whose hits are all current returns the previous output shape with no extra columns.
  Previously duplicated titles now appear once.
- `go test -tags fts5,lancedb ./...` green. `make lint` clean after removing the
  now-unused `currentRevision` and fixing one `appendAssign` in the new test.
