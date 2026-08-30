---
title: Memory importance moves to the frontmatter, and the frontmatter moves to the YAML parser
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [memory, frontmatter, refactor]
---

# Memory importance moves to the frontmatter, and the frontmatter moves to the YAML parser

## Objective

A memory is marked important today by a **naming convention**: the file is written as
`<id>_important_.md` instead of `<id>.md`. The Engineer's instruction is that this makes no
sense and that the YAML frontmatter is where the flag belongs.

It is already there. `renderMemoryFile` writes `important: true` into the frontmatter and
`ParseMemoryFrontmatter` reads it back through `reFrontmatterImportant`. So the state is
encoded **twice**, and the two encodings are not equal: the filename is treated as
authoritative (`updatedMemoryContent` overwrites `fm.Important` with what the filename said,
with a comment saying so), while the frontmatter is the copy that silently drifts. The
drift is measurable, not theoretical — in this machine's own project store, **12 of 186**
`*_important_.md` files carry no `important: true` line at all: they were promoted before
`withImportantFlag` existed, when promotion only moved bytes between two filenames.

The goal is one encoding: **the frontmatter**. The filename becomes `<id>.md` for every
memory, importance is read from the parsed frontmatter everywhere, and promote/demote stop
being a rename.

No backward compatibility is required — the Engineer stated we are in dev. Existing stores on
this machine are handled by a one-off migration run by hand (T6), not by compatibility code
left in the source.

## Plan & Task Breakdown

- [x] **T1 — One filename, one importance reader** — Spec: `internal/memory/important.go`.
  Delete `ImportantMemorySuffix`, `IsImportantMemory`, `ImportantFileName`, `NormalFileName`.
  Add `MemoryFileName(id)` (`<id>.md`), `MemoryIDFromFileName(name)` and
  `IsImportantContent(content)` reading `ParseMemoryFrontmatter(...).Important`. Rewrite
  `listImportantInDir` to read every `.md` and filter on the parsed flag. Done when nothing
  in the package derives importance from a path.

- [x] **T2 — Writers stop choosing a filename** — Spec: `internal/memory/memory.go`
  (`AddMemory`, `updateMemory`, `RemoveMemory`, `archiveBeforeDelete`, `ListMemories`).
  A memory has exactly one path, `MemoryFileName(id)`. `updatedMemoryContent` must no longer
  be told what importance is: it preserves what the old content declared. Constraint: the
  archive-before-write ordering in `updateMemory`/`RemoveMemory` must not change — history
  is written before the file it replaces.

- [x] **T3 — Promote/demote rewrite in place** — Spec: `changeRelevance` in
  `internal/memory/memory.go`. Read `<id>.md`, re-render with `withImportantFlag`, write the
  same path. No rename, no `RemoveFile`. Constraint: promotion is not an edit of content —
  `updated_at` and `revision` must stay as they were, which is what `withImportantFlag`
  already guarantees.

- [x] **T4 — The other two readers** — Spec: `internal/memory/wiki.go` (`GenerateMemoryWiki`)
  and `internal/memory/consolidate.go` (`loadMemorySnapshots`). Both currently derive `id` and
  `important` from the filename. Both already read the file's bytes, so both take the flag
  from `ParseMemoryFrontmatter` and the id from the name minus `.md`.

- [x] **T5 — User-facing text** — Spec: `cmd/graphit/commands/memory.go`. The `important`,
  `promote` and `demote` help strings describe renaming files and matching `*_important_.md`.
  They must describe the frontmatter flag instead.

- [x] **T6 — Migrate the stores on this machine** — Spec: every
  `~/.graphit/memory-raw/memory-*/` directory. For each `*_important_.md`: ensure
  `important: true` is in the frontmatter (12 files lack it), then rename to `<id>.md`.
  Constraint: back up first; the flag must be inserted inside the frontmatter block, after
  `type:` where present, never appended to the body.

- [x] **T8 — Frontmatter through the YAML parser, both ways** — Spec:
  `internal/memory/memory.go`. `ParseMemoryFrontmatter` reads the block with
  `wiki.FrontmatterBlock` + `yaml.Unmarshal`; `renderMemoryFile` writes it with
  `yaml.Marshal` over a tagged `MemoryFrontmatter`. Delete the twelve `reFrontmatter*`
  regexes. Constraint: the field order on disk is the struct's field order, so the struct is
  reordered to match what was being written by hand.

- [x] **T7 — Tests and verification** — Spec: `internal/memory/*_test.go`. The suffix is
  asserted in ~30 places; rewrite them against the frontmatter. `go build ./...` and
  `go test ./internal/memory/... ./cmd/...` must pass.

## Implementation Details

### T1 — `internal/memory/important.go`

`ImportantMemorySuffix`, `IsImportantMemory`, `ImportantFileName` and `NormalFileName` are
gone. What replaced them:

| new | what it does |
|---|---|
| `MemoryFileName(id)` | `<id>.md` — the only filename a memory has |
| `MemoryIDFromFileName(name)` | base name minus `.md` |
| `IsImportantContent(content)` | `ParseMemoryFrontmatter(content).Important` |

`listImportantInDir` now reads every `.md` in the directory and keeps the ones whose parsed
frontmatter says `important: true`. It costs a read per memory instead of a read per
important memory; the directory was already being fully listed, and the caller (the IDE rule
block, `memory important`) is not a hot path.

### T2/T3 — `internal/memory/memory.go`

- `AddMemory` writes `MemoryFileName(id)` regardless of `opts.Important`.
- `updateMemory` and `RemoveMemory` no longer probe two candidate paths.
  `archiveBeforeDelete` takes the single path.
- `memoryUpdate.Important` was deleted. `updatedMemoryContent` used to overwrite
  `fm.Important` from it; now the value parsed out of the old content stands, which is the
  whole point of the change.
- `changeRelevance` reads the file, writes `withImportantFlag(data, promote)` back to the
  same path, and publishes. Asking for a state the memory is already in is a no-op that
  returns `nil` — with one encoding there is nothing to repair, so there is nothing to
  report. The old "not found (or already important)" wording existed because a missing file
  was genuinely ambiguous between those two cases; it no longer is.

### T4 — readers

`GenerateMemoryWiki` and `loadMemorySnapshots` both had the same three-line shape (test the
name, then trim one of two suffixes). Both now take `id` from `MemoryIDFromFileName` and
`important` from the frontmatter they were already parsing.

### T6 — migration performed on this machine

```
backup:                        ~/.graphit/memory-raw.bak-20260830/
scanned:                       34 scope directories, 257 memory files
rewritten with valid YAML:     257
renamed <id>_important_.md ->  198
backfilled `important: true`:  12   (promoted before withImportantFlag existed)
recovered from invalid YAML:   125  (unquoted `: ` or backtick in the title)
timestamps restored to RFC3339: 132 files, 264 lines (second pass — see System Knowledge)

verification: 0 invalid YAML, 0 id/filename mismatches, 198 important
```

The `.wiki/` shard mirror inside each raw directory is keyed by source filename, so the
renames invalidate those cache entries. That is inert by design — a shard whose key does not
match is never read — and the next compile rebuilds them.

### T8 — the frontmatter goes through the YAML parser

The Engineer's second instruction, mid-task: *"para frontmatter sempre use o parser de yaml
para evitar problemas com encode decode, assim como já é usado na wiki. o parser foi feito
para esse trabalho."*

Both halves were hand-rolled:

- **Reading**: twelve `regexp` patterns, each `(?m)^field:\s*(.+)$`, matched against the
  **whole file**. Not against the frontmatter block — the whole file. So a memory whose body
  contained the line `important: true` read back as important. The new
  `TestIsImportantContent/word_in_body_only` case caught exactly that before the rewrite.
- **Writing**: `fmt.Fprintf(&b, "title: %s\n", ...)`, which emits invalid YAML for any title
  containing `": "`, a leading backtick, or a leading `[`.

Both are now the parser's job: `ParseMemoryFrontmatter` extracts the block with
`wiki.FrontmatterBlock` and unmarshals it into the struct; `renderMemoryFile` marshals the
struct. `MemoryFrontmatter` gained `yaml:"..."` tags and its fields were reordered to the
order they were previously written in, because with `yaml.Marshal` the struct's field order
IS the on-disk order. `tags` keeps its flow style (`tags: [a, b]`) via `yaml:"tags,flow"`.

**How broken the store actually was:** of the 257 memory files on this machine, **125 had
frontmatter that does not parse as YAML at all** — nearly half. Every one of them was a title
with an unquoted colon or backtick. Under the old regex reader that was invisible; under a
parser it would have been 125 memories with no title, type or tags. The migration (T6) is
what closes that: it recovers those fields with the legacy line scan and re-emits them
correctly quoted, so the parser-only reader is safe.

## Use Cases

### UC-01: Insert a memory marked important
- **Actor**: agent (MCP `memory_insert`) or user (`graphit memory insert --important`).
- **Preconditions**: the memory store for the scope is configured.
- **Main Flow**:
  1. `MemoryAppService` → `MemoryService.AddMemory(title, body, MemoryOpts{Important: true})`.
  2. `buildMemoryFile` renders frontmatter with `important: true`.
  3. The file is written to `MemoryFileName(id)` — `<ULID>.md`.
  4. `syncToLocalFast` recompiles the wiki, which reads the flag from the frontmatter.
- **Alternative Flows**: `Important: false` renders no `important:` line at all; absence is
  the false value.
- **Error Scenarios**: store not configured → `memory repository not configured — run
  'graphit setup' first`; write failure → `writing memory file: <err>`.
- **Postconditions**: exactly one file exists for the id, and its frontmatter is the only
  record of its importance.
- **Affected Files**: `internal/memory/memory.go`, `internal/memory/important.go`.

### UC-02: Promote or demote an existing memory
- **Actor**: agent (MCP `memory_promote`/`memory_demote`) or user (`graphit memory promote <id>`).
- **Preconditions**: `<id>.md` exists in the scope.
- **Main Flow**:
  1. `PromoteMemory`/`DemoteMemory` → `changeRelevance(id, promote)`.
  2. The file is read from `MemoryFileName(id)`.
  3. `withImportantFlag` re-renders it with the flag set or cleared, preserving body,
     `updated_at`, `revision` and every other field.
  4. The same path is written back and published.
- **Alternative Flows**: the memory is already in the requested state → no write, no publish,
  `nil` returned.
- **Error Scenarios**: file missing → `memory "<id>" not found`; read/write failure →
  wrapped error, file left untouched.
- **Postconditions**: importance changed; the memory's id, path, revision and content did not.
- **Affected Files**: `internal/memory/memory.go`.

### UC-03: List important memories for the IDE rule block
- **Actor**: the rule renderer (`internal/ast/parser.go`), `graphit memory important`, MCP
  `memory_important`.
- **Preconditions**: the scope's raw directory exists.
- **Main Flow**:
  1. `ListImportantMemories(scope)` → `listImportantInDir(RawDir(scope))`.
  2. Every `.md` in the directory (`history/` and `.wiki/` are subdirectories and skipped) is
     read and parsed.
  3. Entries whose frontmatter says `important: true` are returned with id, title and body.
- **Alternative Flows**: directory absent → empty list, no error.
- **Error Scenarios**: an unreadable file is skipped rather than failing the listing.
- **Postconditions**: none — read-only.
- **Affected Files**: `internal/memory/important.go`.

### UC-04: Compile the memory wiki
- **Actor**: the daemon, or any write path via `syncToLocalFast`.
- **Preconditions**: raw directory populated.
- **Main Flow**:
  1. `GenerateMemoryWiki` lists the raw directory.
  2. For each memory file it reads the bytes, takes `id` from the filename and `important`
     from the parsed frontmatter.
  3. Important memories are rendered into the wiki index's important section and marked on
     their own page.
- **Error Scenarios**: unreadable file → skipped, compile continues.
- **Affected Files**: `internal/memory/wiki.go`.

## Test Cases & Acceptance Criteria

### Feature: Importance is read from the frontmatter
Ref: UC-01, UC-03

#### Scenario: A memory written as important is listed as important
```gherkin
Given a memory store with no memories
When a memory is inserted with important set to true
Then a file named "<id>.md" exists in the scope's raw directory
  And no file whose name contains "_important_" exists
  And its frontmatter contains "important: true"
  And ListImportantMemories returns that memory
```

#### Scenario: A memory written as normal is not listed as important
```gherkin
Given a memory store with no memories
When a memory is inserted with important set to false
Then its frontmatter contains no "important:" line
  And ListImportantMemories returns an empty list
```

### Feature: Promotion does not move or edit the memory
Ref: UC-02

#### Scenario: Promoting sets the flag and leaves the path alone
```gherkin
Given a memory "<id>.md" whose frontmatter has no "important:" line
When the memory is promoted
Then the file "<id>.md" still exists at the same path
  And its frontmatter contains "important: true"
  And its revision and updated_at are unchanged
```

#### Scenario: Demoting clears the flag
```gherkin
Given a memory "<id>.md" whose frontmatter contains "important: true"
When the memory is demoted
Then the file "<id>.md" still exists at the same path
  And its frontmatter contains no "important:" line
```

#### Scenario: Promoting an already-important memory is a no-op
```gherkin
Given a memory "<id>.md" whose frontmatter contains "important: true"
When the memory is promoted
Then no error is returned
  And the file's bytes are unchanged
```

#### Scenario: Promoting a memory that does not exist fails
```gherkin
Given a memory store with no memory under the id "MISSING"
When the memory "MISSING" is promoted
Then an error reporting that the memory was not found is returned
```

### Feature: Importance survives an update
Ref: UC-01, UC-02

#### Scenario: Updating an important memory keeps it important
```gherkin
Given an important memory "<id>.md" of type "correction" tagged "auth,security"
When its body is updated
Then its frontmatter still contains "important: true"
  And its type and tags are unchanged
  And its revision is incremented by one
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/memory/important.go` | Modified | Suffix helpers replaced by frontmatter-based ones |
| `internal/memory/memory.go` | Modified | One path per memory; promote/demote no longer rename |
| `internal/memory/wiki.go` | Modified | Importance read from frontmatter during compile |
| `internal/memory/consolidate.go` | Modified | Snapshot importance read from frontmatter |
| `internal/memory/consolidate_similarity.go` | Modified | `memoryIDFromSource` no longer truncates at the first `_` |
| `internal/memory/memory_update_test.go` | Modified | `memoryUpdate.Important` no longer exists |
| `internal/memory/consolidate_apply_test.go` | Modified | Fake writer has one path per memory |
| `internal/memory/history_test.go` | Modified | `NormalFileName` → `MemoryFileName` |
| `cmd/graphit/commands/memory.go` | Modified | Help text described the rename, not the flag |
| `internal/memory/memory_test.go` | Modified | Suffix assertions rewritten against the frontmatter |
| `internal/memory/memory_coverage_test.go` | Modified | Same |
| `internal/memory/memory_coverage_boost_test.go` | Modified | Same |
| `internal/memory/memory_full_coverage_test.go` | Modified | Same |
| `internal/memory/consolidate_test.go` | Modified | Same |

## Trade-offs & Decisions

- **The whole frontmatter through the parser, not just the importance flag.** Fixing the
  duplication without fixing the encoding would have moved the single source of truth into a
  field that a regex reads out of the body by accident. The two changes are one change.
- **No compatibility path for the 125 unparseable files — they were repaired instead.** A
  reader that fell back to the line scan would have kept working, and would have kept the
  broken files broken forever, since nothing rewrites a memory that is not edited.
- **Frontmatter over filename, rather than filename over frontmatter.** The filename can hold
  one bit and nothing else; the frontmatter already holds nine fields including this one, and
  it is what every reader parses anyway. Keeping the filename would have meant keeping the
  duplication that produced the 12 drifted files.
- **No compatibility shim.** A reader that also accepted `<id>_important_.md` would have made
  the migration optional and the duplication permanent — and the Engineer explicitly ruled out
  backward compatibility. The stores on this machine were migrated by hand instead (T6).
- **`listImportantInDir` now reads every file in the scope.** Filtering on content costs a
  read per memory where filtering on a name cost none. Accepted: the callers are the rule
  renderer and a CLI listing, both of which already read every memory's metadata.
- **Promote-when-already-promoted returns `nil` instead of an error.** The old error existed
  because a missing file could mean either "no such memory" or "already in that state"; with
  one filename that ambiguity is gone, and reporting a state the caller asked for as an error
  is noise.

## Technical Debt

- [x] `renderMemoryFile` built YAML by `fmt.Fprintf` concatenation — resolved by T8.
- [ ] `internal/knowledge/wiki.go` and `internal/memory/wiki.go` still build wiki-page
  frontmatter by concatenation. They are less exposed than the raw writer was, because every
  interpolated value goes through `wiki.YAMLScalar`, which quotes — but the ordering and the
  `confidence: %.2f` / `cluster: %d` formatting are still hand-managed, and a field added
  without `YAMLScalar` reintroduces the bug silently. Same treatment as T8 is the fix.
- [ ] The `.wiki/` shard mirrors in the migrated stores hold entries keyed by the old
  filenames. They are inert (content-hash mismatch is never read) but they are dead weight
  until a compile with reset clears them.
- [ ] **The installed binary is still the old one** (`/usr/local/bin/graphit`, root-owned,
  built before this change), and it is what the daemon and the MCP server run. It wrote the
  memories created during this session as `<id>_important_.md`, which the migration had to
  normalise a second time. Until `make install` (needs sudo) and a daemon restart, every
  memory write reverts to the old format. The Engineer takes this step.
- [ ] **185 duplicate `_2.md` pages in the compiled memory wiki** at
  `~/.graphit/wiki/memory/project/01KSH1CRFFG8Z74B5ZS78WW808/`. The daemon compiled the store
  mid-migration, when a memory existed under both names, and `UniqueSlug` gave the second one
  a `_2` slug. `GenerateMemoryWiki` prunes pages absent from `keepFiles`, so ONE clean
  recompile removes them — but it must run with the NEW binary: the old reader takes
  importance from the filename, so recompiling with it would drop all 198 important marks
  from the wiki and from the IDE rule block.

## System Knowledge

- **The regex frontmatter reader matched against the whole file, not the frontmatter.**
  That is why `important: true` in a body promoted a memory. Any future "read a field out of
  a markdown file" helper must extract the block first — `wiki.FrontmatterBlock` does it.
- **Half the memory store had frontmatter that no YAML parser would accept** (125 of 257),
  and nothing reported it, because the only reader was a regex that did not care. The lesson
  generalises: a hand-rolled reader paired with a hand-rolled writer validates nothing, and
  the damage only becomes visible the day a real parser is introduced.
- **PyYAML resolves an unquoted RFC3339 timestamp to a `datetime`**, so the first migration
  pass rewrote `created_at: 2026-07-29T13:58:48Z` as `2026-07-29 13:58:54+00:00`. Caught by
  running `graphit memory list` against the migrated store and reading the dates. A second
  pass restored RFC3339. If this migration is ever re-run, load with a loader that keeps
  scalars as strings.
- **Importance was encoded twice, and the filename was declared the winner.**
  `updatedMemoryContent` had an explicit comment — "The filename is authoritative for
  importance" — which is why the frontmatter copy was allowed to drift. Any future duplicated
  state in this package should be read as the same bug waiting to happen.
- **`withImportantFlag` was added to patch exactly this.** It re-renders the file with the
  flag corrected, without touching `updated_at`, because promotion is not an edit. It survives
  the refactor as the single writer of the flag.
- **`history/` and `.wiki/` are subdirectories, and every listing in the package reads one
  level with `os.ReadDir` and skips directories.** That is what keeps archived revisions and
  shard mirrors from being seen as memories. Switching any of those listings to `WalkDir`
  would resurrect that bug — it is fixed by `TestArchivedRevisionsAreNotMemories`.
- **The daemon's MCP tools were unresponsive during this session** (two calls hung and were
  cancelled), so the exploration after that point used the shell directly.

## Progress Log

### 2026-08-30
- Searched memory first: found the frontmatter-writer bug memory (hand-built YAML, invalid on
  titles containing `": "`) and the "Git ZERO na memória" decision that put `revision`,
  `previous` and `updated_by` in the frontmatter — the same direction this task continues.
- Mapped every use of the suffix: 4 helpers in `important.go`, and readers in `memory.go`,
  `wiki.go`, `consolidate.go`, plus CLI help text. `internal/ast/parser.go`,
  `internal/mcpstdio/tools_memory.go` and `cmd/graphit/commands/runners.go` only call
  `ListImportantMemories`, so they need no change.
- Measured the drift in the real store before touching code: 12 of 186 important files had no
  `important: true` line — the concrete evidence that the duplicated encoding was already
  broken.
- The Engineer interrupted to say no backward compatibility is needed (dev), and that the MCP
  calls appeared frozen. Plan adjusted: delete the suffix outright rather than keep a reader
  for it, and continue the exploration through the shell.
- T1–T5 landed: the suffix and its four helpers are gone; `AddMemory`, `updateMemory`,
  `RemoveMemory`, `ListMemories`, `GenerateMemoryWiki` and `loadMemorySnapshots` all take
  importance from the parsed frontmatter; `changeRelevance` rewrites in place; CLI help text
  now describes the flag instead of a rename.
- T6 ran against `~/.graphit/memory-raw/` after backing it up to `memory-raw.bak-20260830/`:
  12 files backfilled with `important: true`, 198 files renamed to `<id>.md`, zero
  `*_important_.md` left.
- T7: test files rewritten against the frontmatter. Two of the new tests failed on the first
  run, and both were real: `MemoryIDFromFileName("")` returned `"."` (from `filepath.Base`),
  and `IsImportantContent` returned true for `important: true` sitting in the BODY.
- The Engineer then gave the second instruction — always use the YAML parser for frontmatter,
  encode and decode, as the wiki already does. That is T8, and the second failing test above
  was already a symptom of exactly it.
- T8 landed: `ParseMemoryFrontmatter` now unmarshals the extracted block, `renderMemoryFile`
  marshals the tagged struct, and the twelve `reFrontmatter*` regexes are gone.
- T6 re-ran with the parser in mind: 257 files rewritten, 198 renamed, 12 important flags
  backfilled, and **125 files whose frontmatter was invalid YAML recovered** via the legacy
  line scan and re-emitted correctly quoted. Verified: 0 invalid, 0 id/filename mismatches,
  198 important. `graphit memory list` and `graphit memory important` read the migrated store
  correctly.
- Full suite green: `go build ./...`, `go test ./...` — no failures. Committed on `main` as
  `refactor(memory): importance lives in the frontmatter, and the frontmatter goes through
  the YAML parser`.
- Found afterwards, while writing new memories: the MCP server and the daemon still run the
  binary installed at `/usr/local/bin/graphit`, which predates this change — so the two
  memories written in this session came back as `<id>_important_.md` with hand-built
  frontmatter, and the migration had to be re-run over the store. Also left behind: 185
  duplicate wiki pages from a mid-migration compile. Both are in Technical Debt; the
  Engineer runs `make install` (sudo) and restarts the daemon, and the recompile that
  follows clears the duplicates.
- No `graphit_sync` was run at the end of this session, deliberately. With the old binary
  still installed, a recompile of the memory wiki would read importance from filenames that
  no longer carry it and silently drop all 198 important marks. The sync belongs after the
  install, not before it.
- Self-inflicted, recorded because the trap is already a memory in this store: `graphit
  config get memory.store` does NOT read a key — the CLI is `graphit config <key> <value>`,
  so it WROTE a junk key `get: memory.store` into `graphit.lock.json`. Reverted with `git
  checkout graphit.lock.json`. The reading form is `graphit config --get <key>`.
