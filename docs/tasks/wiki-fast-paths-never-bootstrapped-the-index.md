---
title: Wiki fast paths never bootstrapped the index, and didn't see new files
status: done
created: 2026-08-05
updated: 2026-08-05
tags: [wiki, memory, knowledge, fts, incremental, bug]
---

# Wiki fast paths never bootstrapped the index, and didn't see new files

## Objective

`graphit_memory_search` returned `results[0]` for any term in this project, while
`memory_list`, `memory_important` and `memory_schema` responded normally. The store had 26
memories (18 marked important); the index had **zero chunks and zero generated pages**.
`graphit_memory_index` responded `"Memory indexing completed"` without writing anything — a silent
no-op. Practical consequence: every previous session ran without recall, because the session
start protocol called `memory_search`, got zero, and continued as if the project had no
memory at all.

The goal was to find the cause, fix it in code, and prove with tests that the symptom doesn't
return. During the investigation the same defect was confirmed in the knowledge wiki.

## Implementation Details

The cause is two independent defects in the shared helpers of `internal/wiki`,
both exercised by `GenerateMemoryWiki` and `GenerateKnowledgeWiki`.

### Defect 1 — `FastPathCheck` passed through empty on a pristine wiki dir

`wiki.FastPathCheck` (`internal/wiki/fastpath.go`) required three conditions, and on a pristine
directory all three were satisfied **by emptiness**:

1. `wiki.db` must exist — and it did, empty. Who creates it is `OpenWikiDB`, which runs
   `migrateWikiSchema` on any open, including a read. In other words: the first
   memory search, done before any memory existed, created the empty database and thus
   armed the trap for all subsequent runs.
2. No entry with a changed hash in `processCache` — and none had changed, because
   `GenerateMemoryWiki` itself does `processCache.Store` for every doc **before** calling
   `FastPathCheck`. The cache is queried by the same pass that just populated it.
3. No orphan `.md` pages — the loop iterated over the wikiDir's `.md` files, and there were none,
   so it passed trivially.

Result: early return, no pages written, `wiki.RebuildDB` never called. The slug check was
unidirectional (pages on disk ⊆ entries); it became bidirectional
(set equality), via size comparison plus an inclusion. Zero additional I/O cost:
the `os.ReadDir` of wikiDir was already done, only read in one direction.

The fallback path for `processCache == nil` in `internal/knowledge/wiki.go` already had
that guarantee — it reads `content_hash` from the page's frontmatter, and a missing page returns
`""`, which doesn't match the hash. The guarantee was lost only in the `FastPathCheck` path,
which confirms the fix restores the intended behavior rather than inventing a new rule.

### Defect 2 — `StatPreCheck` didn't see new files

`wiki.StatPreCheck` (`internal/wiki/statcheck.go`) iterates exclusively over
`cache.AllStatEntries()`, i.e., files **already cataloged**. It never lists the source
directory, so a file the cache has never seen is invisible to it. With one unchanged cache
entry and `wiki.db` existing, it returned `true` and the generator aborted on the first
line. The 24 memories written after the first run were never even considered.

Deletion was already detected (Phase B tries to `ReadFile` the missing file and fails); addition
was not. Added `StatPreCheckOpts.CurrentSourceFiles func() []string`: when
provided, the pre-check compares the set of existing source files with the cataloged set
and returns `false` for any unknown key. The comparison is of sets, not counts — a rename keeps the count identical.

The variadic signature `watchFiles ...string` became the `WatchFiles` field of the same struct,
so there aren't two configuration mechanisms in the same function.

### Wiring of the two callers

- **memory** (`internal/memory/wiki.go`): the store is a flat directory, so the full
  enumerator is an `os.ReadDir`. The filter that distinguishes a memory source file from a
  generated wiki artifact was extracted into `isMemorySourceFile`, used both by the enumerator
  (`memorySourceFileNames`) and the generation loop, so the rule exists in one place.
- **knowledge** (`internal/knowledge/wiki.go`): the source is a tree with ignore rules,
  allowed extensions and size limits, so the enumerator is a `filepath.Walk`. The predicate was
  extracted into `knowledgeSourceFile` and is used by the enumerator's walk
  (`knowledgeSourceFileNames`) and the generator's walk. The inline `1024*1024` became
  `maxKnowledgeDocBytes`.

## Use Cases

### UC-01: First indexing on a wiki dir where an empty wiki.db already exists
- **Actor**: agent or daemon calling `graphit_memory_index` / `graphit_sync`
- **Preconditions**: memories exist in the store; `wiki.db` exists and is empty because a
  read path created it; no `.md` pages have been generated yet
- **Main Flow**:
  1. `MemoryService.IndexMemories` calls `GenerateMemoryWiki`
  2. `wiki.StatPreCheck` returns `false` because the cache is empty (`AllStatEntries` nil)
  3. each memory is read, parsed and stored in `processCache`
  4. `wiki.FastPathCheck` returns `false` because no entry has a page on disk
  5. one page per memory is written; `wiki.RebuildDB` populates `chunks` and `chunks_fts`
- **Alternative Flows**:
  - empty store: `entries` empty, `newSlugs` and `existingSlugs` empty, nothing to do
- **Error Scenarios**:
  - `os.ReadDir` of wikiDir fails in `FastPathCheck` ⇒ `false`, the full pipeline runs
  - `wiki.db` missing ⇒ `false` on condition (1), the full pipeline runs
- **Postconditions**: `count(chunks)` equals the number of memories; `memory_search` ranks
- **Affected Files**: `internal/wiki/fastpath.go`, `internal/memory/wiki.go`

### UC-02: Memory added after a successful indexing
- **Actor**: agent calling `graphit_memory_insert`, followed by the reindex cycle
- **Preconditions**: the wiki is complete and in sync; the new memory is the only
  file changed in the store; every old memory has its mtime+size intact
- **Main Flow**:
  1. `GenerateMemoryWiki` calls `wiki.StatPreCheck` with `CurrentSourceFiles`
  2. the enumerator lists the store and returns N+1 names vs N cataloged keys
  3. the pre-check returns `false` because of the unknown key
  4. `FastPathCheck` returns `false` because the new memory has no page
  5. the new page is written, `RebuildDB` reindexes with N+1 chunks
- **Alternative Flows**:
  - memory removed: Phase B fails on `ReadFile` of the missing file and returns `false`
  - memory renamed: same count, but the new key is not in the cache ⇒ `false`
  - nothing changed: identical sets, Phase A matches everything, `true` and the rebuild is skipped
- **Error Scenarios**:
  - `os.ReadDir` of the store fails in the enumerator ⇒ nil list ⇒ count diverges ⇒ `false`
    (fails toward reindexing, never toward skipping)
- **Postconditions**: the new memory is findable via `memory_search`
- **Affected Files**: `internal/wiki/statcheck.go`, `internal/memory/wiki.go`

### UC-03: Document added to the docs tree without touching any existing document
- **Actor**: agent or human creating a file under the docs tree
- **Preconditions**: the knowledge wiki is in sync and every manifest entry has its
  mtime+size recorded
- **Main Flow**:
  1. `GenerateKnowledgeWiki` calls `wiki.StatPreCheck` with `CurrentSourceFiles`
  2. `knowledgeSourceFileNames` walks applying ignore, extensions and size limit
  3. the new document is not in the cache ⇒ `false` ⇒ the full pipeline runs
- **Alternative Flows**:
  - file above `maxKnowledgeDocBytes`, disallowed extension or ignored: not included
    in the enumeration, exactly as not included in the generator's walk, so it doesn't force a rebuild
- **Error Scenarios**:
  - walk error in a subdirectory: ignored per entry, the walk continues
- **Postconditions**: the new document has a page and is findable
- **Affected Files**: `internal/knowledge/wiki.go`, `internal/wiki/statcheck.go`

## Test Cases & Acceptance Criteria

### Feature: wiki index bootstrap
Ref: UC-01

#### Scenario: pristine wiki dir with empty wiki.db already on disk
```gherkin
Given a memory in the store and no generated pages
  And an empty wiki.db created by a read path
When GenerateMemoryWiki runs
Then ArticlesWritten is 1
  And wiki.db now contains 1 chunk
```

#### Scenario: present page and unchanged hash allows skipping the rebuild
```gherkin
Given a cataloged entry whose hash hasn't changed
  And the corresponding .md page on disk
When FastPathCheck is queried
Then it returns true
```

#### Scenario: page without a corresponding entry forces a rebuild
```gherkin
Given a cataloged entry with its page on disk
  And an orphan page that no entry produces
When FastPathCheck is queried
Then it returns false
```

### Feature: new source file detection
Ref: UC-02, UC-03

#### Scenario: memory added after the first run is indexed
```gherkin
Given a memory wiki in sync with 1 memory
When a second memory is written without touching the first
  And GenerateMemoryWiki runs again
Then wiki.db now contains 2 chunks
  And a search for the new memory's term returns a result
```

#### Scenario: document added to the docs tree is indexed
```gherkin
Given a knowledge wiki built from one document
When a second document is created without touching the first
  And GenerateKnowledgeWiki runs again
Then a search for a term exclusive to the new document returns a result
```

#### Scenario Outline: the pre-check only allows skipping when sets match
```gherkin
Given a cache with the single source "known.md" stat-unchanged
When StatPreCheck receives "<enumeration>" as current sources
Then it returns "<result>"

Examples:
  | enumeration              | result |
  | known.md                 | true      |
  | known.md, brand-new.md   | false     |
  | renamed.md               | false     |
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/wiki/fastpath.go` | Modified | slug comparison became bidirectional; entry without a page now forces generation |
| `internal/wiki/statcheck.go` | Modified | new `StatPreCheckOpts` with `CurrentSourceFiles`; variadic `watchFiles` became a field |
| `internal/memory/wiki.go` | Modified | `memorySourceFileNames` enumerator + shared `isMemorySourceFile` predicate; slug resolved once in `slugs` slice |
| `internal/knowledge/wiki.go` | Modified | `knowledgeSourceFile` predicate extracted; `enumerateKnowledgeSources` walks once and feeds pre-check and generator; `maxKnowledgeDocBytes` |
| `internal/knowledge/staleness.go` | Modified | `Manifest.DocsModTime` and `DocsFileCount` removed — written and never read |
| `internal/memory/rule.go` | Modified | preview warning in step 1 of "Retrieval steps"; search table line fixed |
| `internal/knowledge/rule.go` | Modified | preview warning in both "Step 1 — Search the wiki" blocks (docs and integrations) |
| `.claude/`, `.kiro/`, `.agents/` `skills/graphit-{memory,knowledge}/SKILL.md` | Regenerated | output of `InstallRule`/`InstallSkill` for the three IDEs in the lockfile |
| `internal/wiki/reindex_completeness_test.go` | Created | unit tests for both helpers, including the rename case |
| `internal/memory/wiki_reindex_completeness_test.go` | Created | end-to-end regression: cold start with empty DB, and memory added afterwards |
| `internal/knowledge/wiki_reindex_completeness_test.go` | Created | end-to-end regression: document added without touching existing ones |

## Trade-offs & Decisions

- **Bidirectional instead of checking the DB content.** The measured symptom was `chunks = 0`,
  so checking `count(chunks)` inside `FastPathCheck` would be the most direct test. It was
  discarded: it would require opening SQLite inside a path whose purpose is to be almost
  free, and `OpenWikiDB` runs schema migration on open. The set comparison
  uses the `os.ReadDir` that was already done and costs zero additional I/O.

- **Per-caller enumerator instead of generic in the helper.** `StatPreCheck` cannot know what
  counts as a source file: memory is a flat `.md` directory, knowledge is a tree with ignore,
  extension allowlist and size limit. A generic enumerator would classify as "not cataloged" every
  file the generator intentionally discards, and the fast path would die — correct, but always slow.
  The callback leaves the decision where the filter lives.

- **`CurrentSourceFiles` optional.** With `nil` the old behavior is preserved, which
  maintains compatibility and makes the gap explicit in the field's documentation instead of
  implicit in code. Both callers in this repo pass the enumerator.

- **The extra walk for knowledge was accepted.** The knowledge enumerator does a `filepath.Walk`
  per call, including when nothing changed, and when the pre-check returns `false` the generator
  walks again. Measured on this machine: 64,237 files in 37 ms. The 2026-06-18 decision
  that introduced the pre-check (`Wiki Reindex Performance Fix`) measured 4.3 s → 0.0 s, and that
  cost was `ReadFile` of ~199 documents, hashing, chunking and FTS rebuild — not the walk.
  Preserving the skip of the expensive part and paying for the walk is the right trade; a silently
  broken correctness is worse than tens of milliseconds.

- **Set comparison, not count.** Count alone would miss a rename, which is a normal operation in the
  memory store (the `_important_` suffix comes and goes in `promote`/`demote`). The rename test exists
  for that reason.

## Technical Debt

- [x] Knowledge paid for an extra walk per call, and two when something changed. Resolved
      with `enumerateKnowledgeSources`, which walks the tree once and returns
      `[]knowledgeSource{relPath, ext, mtime, size}`. The pre-check receives the relPaths via
      `knowledgeSourceRelPaths` closing over the already computed slice, and the generator iterated
      from a list loop instead of an inline walk. Left with ONE `filepath.Walk` in the entire
      package, with a single call site.
- [x] `internal/memory/wiki.go` built the chunk slug with `wiki.SafeSlug(doc.title)` without
      `wiki.UniqueSlug`, while the page used the unique version. Resolved by resolving the slug
      ONCE per doc in a `slugs` slice reused by the three consumers (fastEntries, page
      writing, chunks). Measured before the fix: `chunks = 2` but `COUNT(DISTINCT slug) = 1` —
      the two memories coexisted as two rows with the same slug, not one overwriting the
      other.
- [x] `Manifest.DocsFileCount` and `DocsModTime` (`internal/knowledge/staleness.go`) were
      written in two places and read in none. Removed, along with the assignments. They
      couldn't become a useful pre-check: count and max-mtime only exist AFTER enumerating, which is
      precisely what a pre-check would need to avoid. `SourceHashes` and `PageSources` remain —
      `DetectStalePages` uses them.
- [ ] No path reports a mutilated index. `count(chunks) = 0` with a non-empty store is a
      detectable and diagnosable state, and today it only appears as a search that returns nothing.
- [x] Search results didn't announce they were partial. The snippet is the chunk body cut
      to 200 chars by `truncateSnippet` (`internal/wiki/fts.go`), and `memory_search` and
      `knowledge_search` share exactly that path — both call `wiki.BM25Search`. The skills already
      told the reader to read the page with `wiki_source` (step 3 in memory, step 1b in knowledge),
      but nothing said the hit was truncated, so the agent had no trigger to take that step. Resolved
      with a warning in step 1 of both rules, plus the memory table line that said only "~200 tokens".
      In knowledge the warning went into BOTH "Step 1 — Search the wiki" blocks: the one for documentation
      and the one for integrations, where the text is more specific (never read a field name, type, endpoint
      path or error code from the preview). Returning the full body was discarded:
      `top_k` omitted means no limit (`0 = no limit` in `internal/mcpstdio/tools_memory.go`), so a search
      in this project's current state would bring 27 full memories in one result, and the session start
      protocol does a search before the first response.
- [ ] Whether the 200-char budget should be larger ONLY for memory remains open. The knowledge
      chunk is already a document piece (split by H2), so 200 chars cut a section; the memory chunk is a
      whole memory, so the cut is proportionally much more aggressive. `truncateSnippet` is called from
      `queryChunksFTS`, which doesn't know which wiki it came from — separating would require parameterizing the limit.

## System Knowledge

- **`OpenWikiDB` creates.** Any open, including a read during a search, does `MkdirAll`,
  writes the `.gitignore` and runs `migrateWikiSchema`. An existing `wiki.db` is not evidence
  that something was indexed — it is evidence that someone opened the file. That is what armed
  defect 1: the `.gitignore` and `wiki.db` timestamps in the affected directory were identical
  and prior to the first memory.

- **The cache is populated before it is queried.** In both generators, `processCache.Store`
  runs in the processing loop and `FastPathCheck` runs afterwards. Any future fast-path condition
  that relies only on the cache is vacuously true on the first run. Evidence outside the cache
  — page on disk, file on disk — is what counts.

- **`AllStatEntries` returns nil if any entry is missing mtime or size**, and nil makes
  `StatPreCheck` return `false`. A partially populated cache therefore disables the
  entire pre-check, which masks defect 2: while any entry is incomplete, new files are
  detected by accident.

- **Knowledge escaped via correlated edits.** A new document almost always arrives together
  with a change to an existing document (changelog, index), and a change to a cataloged file
  drops the pre-check. The defect was there since 2026-06-18 and only appears when the
  addition is isolated — which is exactly what the new test does.

- **Symptom to recognize again:** `memory_list` responds and `memory_search` returns zero.
  That separates the store from the index. The next step is `select count(*) from chunks` in the
  `wiki.db` of the corresponding wiki dir, not investigating the store.

## Progress Log

### 2026-08-05
- Symptom reported: `memory_search` with zero results for any term.
- Confirmed that `list`, `important` and `schema` worked, and that the knowledge wiki of
  the same project responded — ruling out daemon, FTS and environment.
- Measured the on-disk state: `chunks = 0`, zero pages, manifest with a single entry from an
  old first run, 26 memories in the store.
- Isolated in three steps: clearing manifest+shards and reindexing recataloged everything but kept
  `chunks = 0` (defect 1 exposed); clearing also the empty `wiki.db` unlocked the full
  bootstrap; the memory inserted afterwards didn't enter the index (defect 2).
- Verified previous decisions in memory and the wiki before touching anything: the fast paths
  exist for a measured performance task, so the fix preserves the skip.
- Implemented both fixes and the three test files. The tests were run with the new guards
  neutralized and failed reproducing the production symptom, including `indexed chunks = 0`; with
  the guards active, they pass.
- Suites for `internal/wiki`, `internal/memory`, `internal/knowledge`, `internal/mcpstdio`,
  `internal/wikisvc` and `internal/daemon` green with `-tags fts5`.
- Live index of the project restored by clearing the derived artifact: 27 memories,
  27 pages, 27 chunks, search ranking.
- Pending for the human: `make install` needs sudo and couldn't run in this session, so
  the running MCP still loads the old core. Until installed, new memory will again not be
  indexed and the wiki dir cleanup has to be repeated.

### 2026-08-05 (second pass — debts)
- Closed the first three debts in this list: single walk for knowledge, single slug resolution
  for memory, and removal of the two dead `Manifest` fields.
- Added `TestGenerateMemoryWiki_MemoriesSharingATitleGetDistinctSlugs`. Verified it
  fails with the old logic (`distinct slugs = 1; want 2`) and passes with the new.
- Confirmed that only one `filepath.Walk` remains in the knowledge package, with a single call site.
- Suites for `internal/wiki`, `internal/memory`, `internal/knowledge`, `internal/mcpstdio`,
  `internal/wikisvc` and `internal/daemon` green with `-tags fts5` after the refactor.
- Raised the new debt about `memory_search` not announcing itself as partial, with the measurement
  that discards the obvious solution (`top_k` without limit by default).

### 2026-08-05 (third pass — preview warning in rules)
- Confirmed that `knowledge_search` and `memory_search` call the SAME `wiki.BM25Search`, hence the
  same `truncateSnippet(body, 200)`. The warning went to both rules, not just memory.
- `internal/memory/rule.go`: warning in step 1, and the table line that promised "~200 tokens"
  now says each hit is a ~200-char preview and never the full memory.
- `internal/knowledge/rule.go`: warning in both "Step 1 — Search the wiki" blocks; the line that
  claimed "The search returns entity summaries" was corrected, because what comes back is a
  body snippet, not a summary.
- Regenerated with the LOCAL code, not the installed binary — the rule text is compiled
  inside the binary, so regenerating with the old core would have rewritten the old text. Done
  with a temporary `main` under `tmp-regen/` calling `memory.InstallRule/InstallSkill` and
  `knowledge.InstallRule/InstallSkill` for `antigravity`, `kiro` and `claude` (the three IDEs in
  `graphit.lock.json`), and removed afterwards.
- `AGENTS.md` didn't change, and that's expected: it loads the short mandate blocks, not the
  retrieval steps. The warning is SKILL.md content.
- Coexistence note: another session was editing `internal/ast/` (rule.go, queries/*.yaml,
  ladybug.go, cache_convert.go) and the copies of `graphit-ast/SKILL.md` during this task.
  No files in common; the regeneration done here only called the memory and knowledge installers.
