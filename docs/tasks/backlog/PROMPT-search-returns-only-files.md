> **RESOLVED on 2026-08-24.** Both defects are fixed and verified through the CLI —
> see `docs/tasks/search-returns-only-files-and-index-not-rebuilt.md`.
>
> **The diagnosis of Defect 1 below IS WRONG, and that was the most valuable finding of the task.**
> The hypothesis that the file IDF dominates the entity IDF did not survive the measurement this
> very prompt ordered to be done first: in the keyword channel, against the real store, the
> entity leads **156.4 against 29.6**. The real causes are (1) score scale — in a hybrid
> query the entity pass returns an RRF sum in hundredths and the file pass raw BM25 in tens —
> and (2) `_score` and `_relevance_score` collapsed in a map iteration in `internal/lancestore`.
> Kept as it is to record the hypothesis and the method that knocked it down.

# Prompt for the next window

I am continuing the Hub migration in `graphit-code`. **Read first** `docs/tasks/hub-on-s3-icebug-and-lancedb.md`
— the "HOW TO CONTINUE" section and T15's entry in the Plan & Task Breakdown — and search memory for
`lancedb` and for `busca`. The most recent relevant commit is `a7c0ac3`.

SQLite is gone entirely; search is LanceDB everywhere. **Two defects were found running the
installed binary (`make install`) against this project's real store, and none of my tests catch
them.** Rules that hold: no search fallback in Go, the engine has priority over anything I would
write, and always measure against a frozen copy of the store, never against what the daemon is
rewriting.

---

## Defect 1 (the serious one): the search returns ONLY files, never entities

### Reproduction

```
graphit ast query --hybrid "evictOldestStaged"
```

`evictOldestStaged` is a function that exists in `internal/hub/s3_store.go`. All five results come
back with `"Type": "File"` — `search_lance_test.go`, `s3_store.go`,
`managed_skills_frontmatter_test.go`, `embedded_selector_test.go`. **Not one entity.**

### Likely diagnosis, not confirmed

`internal/ast/search_lance.go:618`, `func (s *SearchIndex) search`. It runs two queries — one on the
`entities` table, another on `files` — concatenates the results and calls
`sortResultsDeterministic` (`internal/ast/search_common.go:153`), which sorts by
`RelevanceScore` descending.

**The two scores are not comparable.** They are BM25 from two queries over **different corpora**: a
term's IDF depends on document frequency *in that* corpus. A file among ~770
documents has a high IDF; an entity among ~60 thousand, a low one. So the file score dominates by
construction, and the effect only shows up on a large corpus — which is exactly why my tests did not
catch it: they use 2 to 24 entities, where the files table is tiny.

The SQLite index did not have this problem because it merged the passes with explicit weights
(`name_split` 10, `docstring` 3, … and `files.name_split` 8), which normalized across passes. That
merge was deleted on purpose — 331 lines of ranking in Go outside the engine that owns ranking — and
it **must not come back**.

### What to confirm before changing anything

1. That the score really is the cause: log the `RelevanceScore` of both passes for the same query and
   compare the ranges. If the ranges overlap and the problem is something else, the diagnosis above
   is wrong and that is worth more than the fix.
2. That the entity is in the index: `MATCH (n) WHERE n.name = 'evictOldestStaged'` in the graph, and a
   direct query on the `entities` table of `search.lance` filtering by `name`.
3. Whether the file pass ought to run: in `search()` it only runs when
   `len(out) < topK || topK <= 0`. Check whether the entity pass is returning few
   results for another reason — if it returns 0, the cause is upstream of the ranking.

### Possible ways out (pick one, do not stack them)

- **entities first, files as a complement.** It is what the design already says exists ("entities
  first, because a file match and an entity match are different answers"): do not mix the two
  lists in a single sort. Return the entities ordered among themselves, and the files after,
  ordered among themselves, trimming to `topK`. Without comparing scores across corpora.
- **a single table**, with a `kind` field distinguishing file from entity. The engine then ranks
  over one corpus and the scores become genuinely comparable. It is the most correct change and the
  most invasive one: it changes the schema, the rebuild and the incremental.
- **not running the file pass** when the entity one has filled `topK`. It is probably part of the
  fix but does not solve it: with a large `topK` both run again.

Preference of whoever recorded this: the **first** one, and measure the second before discarding it.

### How to know it worked

- `graphit ast query --hybrid "evictOldestStaged"` returns the `evictOldestStaged` entity as the
  first result, with `"Type"` being its label and not `File`.
- **A new test with an UNBALANCED corpus** — many files, few entities per file — which
  fails with the current code. Without that the fix has no guard: the existing tests
  (`internal/ast/search_lance_test.go`, `search_index_test.go`) pass today precisely because the
  corpus is small.
- `TestSearchIndexQualityFloor` stays at **11/11 strict and 5/5 recall**, and
  `TestTruncatedQueryCoverage` at 8/8 plus recall. If any of them goes up, be suspicious: it may be
  the fix masking the metric instead of improving the search.

---

## Defect 2: an existing store ends up with the new index EMPTY and `ast index` does not notice

### Reproduction

On a store indexed before the engine swap (it has `ladybugdb.search.sqlite` and a `search.lance`
of ~44 KB, created and never populated):

```
graphit ast index      # responde "770 files up to date (no changes detected)"
graphit ast query --hybrid "qualquer coisa"   # "No results"
```

`index` compares file hashes, nothing changed, and it does not rebuild. **The search goes
silently empty.**

What saved the manual test was `graphit ast embed`, which **does** have the guard
(`cmd/graphit/commands/ast.go`, `if !ast.SearchIndexBuilt(ctx, idxPath) { … RebuildFromCache }`) —
after it the index went from 44 KB to 241 MB and the search answered.

**The guard is in the wrong command.** Whoever changes version and runs only `ast index` is left
without search and without a warning.

### What to do

Take the same check to the `ast index` path: when nothing changed, still check
`SearchIndexBuilt` and rebuild from the `ShardCache` if the index is absent or empty. The cost is a
count; the benefit is not serving silence.

Watch out for one measured detail: `OpenSearchIndex` **creates** what it opens, so "the directory
exists" says nothing. `SearchIndexBuilt` already checks whether there are rows — use it, not an
`os.Stat`.

### How to know it worked

- A test that: indexes, deletes the `search.lance` directory, runs the "no changes" path, and
  ends with the index populated and the search answering.
- `graphit ast index` on a store whose `search.lance` was removed rebuilds the index instead of
  answering "up to date".

---

## Context that saves time

- **Everything needs `-tags lancedb`** and the native in `.native/` (`make fetch-lancedb`, which requires
  Rust). Without the tag the tree compiles with `ErrNotBuilt` stubs.
- The payload binary needs `make install`; `.build/graphit-local` (core directly) **does not find**
  the grammar YAMLs nor the `httpfs` extension, because both travel in the launcher's payload.
- `graphit setup` is interactive: run it with `< /dev/null` or write `~/.graphit/config.json` directly.
- There is MinIO at `http://localhost:9000` (`admin`/`password`, bucket `graphit-hub`). The engine's TLS
  option is called **`s3_disable_ssl`** — `s3_use_ssl`, `s3_ssl`, `http_use_ssl`, `s3_scheme`,
  `s3_protocol`, `s3_insecure`, `s3_verify_ssl` and `s3_use_tls` all give `Invalid option name`.
- `internal/ast` gives `signal: segmentation fault` **intermittently** under memory pressure. It is a known
  backlog item (buffer pool ceiling without coordination between processes), not a regression.
