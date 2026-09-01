**RESOLVIDO em 2026-08-24.** The two defects have been corrected and verified by the CLI —
see `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`.
>
> **The diagnosis of Defect 1 below IS INCORRECT, and this was the most valuable find in the task.**
> The hypothesis that IDF of file dominates entity IDF did not survive the first measurement made by this very prompt: on the key channel, against the real store, entities lead **156.4 against 29.6**. The true causes are (1) score scale — a hybrid query to pass-through entities returns RRF sums in hundredths and BM25 cru scores in dozens — and (2) `_score` and `_relevance_score` collapsed into an iteration of map on `internal/lancestore`.
> It is kept as it is to record the hypothesis and method that overturned it.

Prompt for the next window

I am continuing the migration of the Hub to `graphit-code`.
**Read first** `docs/tasks/hub-em-s3-icebug-e-lancedb.md`
— read section "How to Continue" and enter T15 into the Plan & Task Breakdown — and search in memory for
`lancedb` and by `busca`. The most recent relevant commit is `a7c0ac3`.

Note: Replace `graphit-code`, `docs/tasks/hub-em-s3-icebug-e-lancedb.md`, `lancedb`, `busca`, and `a7c0ac3` with the actual values or placeholders as needed.

SQLite is intact; the search is LanceDB everywhere. **Two bugs were found running the binary installed (`make install`) against this project's real store, and none of my tests catch them.** Rules that apply: no fallback to Go's query engine, the engine has priority over anything I would write, and measure always against a frozen copy of the store, never against what the daemon is rewriting.

---

Defect 1 (the serious one): The search returns only files, never entities

Reproduction

```
graphit ast query --hybrid "evictOldestStaged"
```

INLINE_10 is a function that exists in INLINE_11. The five results all have INLINE_12 — INLINE_13, INLINE_14, INLINE_15, and INLINE_16. No entity.

Possible diagnosis, not confirmed

And he performs two queries— one on the table `entities`, another on `files` — concatenates the results and calls `sortResultsDeterministic` (`internal/ast/search_common.go:153`), which sorts by `RelevanceScore` in descending order.

The two scores are not comparable. They are BM25 scores from two queries on **different corpora**: the IDF of a term depends on the document frequency *in that* corpus. An archive with ~770 documents has an IDF high; an entity with ~60, 000 documents has a low IDF. Therefore, the score of the archive dominates by construction, and this effect only appears in large corpora—exactly why my tests did not pick up: they use 2 to 24 entities, where the table of archives is small.

The SQLite index did not have this problem because it merged the passes with explicit weights.
(`name_split` 10, `docstring` 3, … and `files.name_split` 8), normalizing between passes. This fusion was purposefully removed — 331 ranking lines in Go outside of the engine that has ranking — and **should not return**.

What needs to be confirmed before making any changes

1. What is the score really causing: log in and compare the bands. If the bands overlap and the problem isn't another one, the diagnosis above is wrong and should be more accurate than the correction.
2. The entity is indexed at `MATCH (n) WHERE n.name = 'evictOldestStaged'` in the graph, and a direct query on table `entities` of `search.lance` filtering by `name`.
3. If the file pass should run: it only runs when `search()`. Check if the entity pass is returning few results for another reason — if it returns 0, the cause is upstream in the ranking.

Possible Outputs (choose one, do not stack)

- **First, entities come first, files as an appendage.** This is what the drawing already says exists ("entities first, because a file match and an entity match are different answers"): do not mix the two lists in one sort only. Return entities ordered among themselves, followed by files ordered among themselves, removing for `topK`. Do not compare scores between corpora.
- **One table, with a field `kind` distinguishing file from entity.** Then the engine ranks on a single corpus and the scores become truly comparable. This is the most correct and invasive change: it changes the schema, rebuilds, and increments.
- **Do not run the pass-through of files when entities fill up `topK`. It probably part of the correction but does not resolve: with `topK` large, both run again.

Preference of who registered: first one, measure the second before discarding it.

How do you know it worked?

- **INLINE_38** returns the entity **INLINE_39** as the first result, with **INLINE_40** being its label and not **INLINE_41**.
- **A new test with an imbalanced corpus** — many files, few entities per file — fails with the current code. Without this correction, the tests do not have a save: existing tests (INLINE_42, INLINE_43) pass today precisely because the corpus is small.
- **INLINE_44** remains at 11/11 strict and 5/5 recall, while **INLINE_45** improves to 8/8 plus recall. If either of them rises, be wary: it might be masking the metric instead of improving the search.


---

Defect 2: An existing store has an empty new index and the `ast index` does not perceive.

Reproduction

In an indexed store before the engine swap (~44 KB inline and a ~47 INLINE),:


```
graphit ast index      # responde "770 files up to date (no changes detected)"
graphit ast query --hybrid "qualquer coisa"   # "No results"
```

The _INLINE_49_ compares file hashes, no changes have been made, and it does not reconstruct. The search is silently empty.

What saved in the manual test was `graphit ast embed`, which **has** the guard
(`cmd/graphit/commands/ast.go`, `if !ast.SearchIndexBuilt(ctx, idxPath) { … RebuildFromCache }`) —
after it, the index went from 44 KB to 241 MB and the search responded.

The guard exists in the wrong command. Whoever replaces versions and runs only `ast index` ends up without a search or an alert.

What to do

Bring the same verification to the path of ___INLINE_54__: when nothing changes, still check ___INLINE_55__ and reconstruct from ___INLINE_56__ if the index is missing or empty. The cost is a count; the benefit is not serving silence.

Be cautious of a measured detail: `OpenSearchIndex` **creates** what opens, so "the directory exists" says nothing. `SearchIndexBuilt` already checks for lines — use it, not a `os.Stat`.

Note: The provided inline codes are placeholders and should be replaced with actual code blocks or content as needed.

How do you know it worked?

A test that: indexes, deletes the directory __INLINE_60__, runs the path of "no changes", and ends with the populated index and the search responding.
- __INLINE_61__ in a store whose __INLINE_62__ has been removed reconstructs the index instead of responding "up to date".

---

Context that saves time

Everything needs `-tags lancedb` and the native in `.native/` (`make fetch-lancedb`, which requires Rust). Without the tag, the tree compiles with stubs `ErrNotBuilt`.
- The payload binary needs `make install`; the `.build/graphit-local` (direct core) does not find the YAML grammars or extension `httpfs` because they travel in the launcher's payload.
- `graphit setup` is interactive: run with `< /dev/null` or type `~/.graphit/config.json` directly.
- MinIO is present at `http://localhost:9000` (`admin`/`password`, bucket `graphit-hub`). The TLS option of the engine is called **`s3_disable_ssl`** — `s3_use_ssl`, `s3_ssl`, `http_use_ssl`, `s3_scheme`, `s3_protocol`, `s3_insecure`, `s3_verify_ssl`, and `s3_use_tls` all give `Invalid option name`.
- `internal/ast` gives `signal: segmentation fault` **intermittent** under memory pressure. It is a known backlog item (tipping point of uncoordinated buffer pool), not regression.

Note: The inline codes are placeholders for actual code blocks, which should be replaced with the appropriate content when translating technical documents or scripts.
