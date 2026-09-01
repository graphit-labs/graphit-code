---
Title: "Search returns only files, and `ast index` does not rebuild an empty index"
status: done
created: 2026-08-24
updated: 2026-08-24
tags: [ast, search, lancedb, ranking, bm25, rrf, index, cli]
---

The search returns only files, and `ast index` does not rebuild an empty index.

Continues `docs/tasks/hub-on-s3-icebug-and-lancedb.md` (T15 closed in `a7c0ac3`). The originating
prompt is at `docs/tasks/backlog/PROMPT-search-returns-only-files.md` (commit `abef386`).

## Objective

Two defects were found while running `make install` against this project's real store: 770 files
and 61,446 entities. None of the test suite caught them.

**Defect 1.** `graphit ast query --hybrid "evictOldestStaged"` returned five results, all
`"Type": "File"`, no entity — despite `evictOldestStaged` being an indexed method in
`internal/hub/s3_store.go`.

**Defect 2.** A store indexed before the engine swap has `search.lance` created but empty.
`graphit ast index` compared hashes, found nothing changed, and reported "up to date" without
rebuilding — the search stayed silently empty. The guard meant to catch this case already existed
(`SearchIndexBuilt` + `OpenSearchIndex`), but it protected the graph rebuild, not the search index
rebuild.

It ended up being four defects: confirming the diagnosis ruled out the original hypothesis and
turned up two more along the way.

### The prompt's diagnosis was wrong — correcting it mattered more than fixing the bug

The prompt claimed both passes run BM25 over corpora of different sizes — a term's IDF over 770
files versus over 61,000+ entities — and that this discrepancy was what made file scores dominate
entity scores by construction. The instruction was to confirm this before changing anything.
Confirmed against a **frozen copy** of the real store (61,446 entities) with `topK = 0`, and the
hypothesis collapses:

```
query "evictOldestStaged", keyword channel
  entity pass : 156.39 evictOldestStaged (Method), 104.31 its doc comment, 53.31, 48.85
  file pass   :  29.63, 24.37, 23.87, 23.16, 22.96
```

The entity scores five times higher in BM25. Corpus size is a false culprit: the entity is a short
document (BM25 normalizes by length and rewards that), and the term is rare among 61 thousand
entities (high IDF). So both effects push the entity's score up, not down — already returning the
entity first, which is exactly why every gate in keyword mode was passing while the CLI returned
only files.

The truth has two halves, and the second one wasn't in the prompt.

### Real cause 1 — score scale: fused vs. raw

In a hybrid query, the entity pass returns the engine's fused score: an RRF sum, on the order of
`1/(60+rank)`, i.e. tenths. The file pass **can't run the vector channel** — the `files` table has
no embedding column — and returns raw BM25, in the hundreds. The concatenation sort then puts every
file above every entity.

This always shows up via the CLI because `--top` defaults to **0 = no limit**, and the file pass
only runs when `len(entities) < topK || topK <= 0` — so the default invocation is exactly the one
that runs both passes.

Reproduced byte-for-byte against the frozen store: the same four files from the report
(`search_lance_test.go`, `s3_store.go`, `managed_skills_frontmatter_test.go`,
`embedded_selector_test.go`), in the same order.

### Real cause 2 — `_score` and `_relevance_score` collapsed by map iteration

Found while investigating why, in the fixture, the order within the entity list was wrong.
`internal/lancestore/store_lancedb.go` built the hit like this:

```go
for k, v := range r {
    switch k {
    case "_score", "_relevance_score":
        h.Score = toFloat(v)
```

These are two different columns, and a hybrid row carries both: `_score` is the text channel's
value (BM25); `_relevance_score` is what the engine's fusion produced by merging the two channels.
With both written into the same `Hit.Score` field, whoever survived was whichever key the map
iteration visited last — and Go randomizes map iteration order.

Measured: twenty identical queries against an unchanged index returned, on every row, two different
scores:

```
row 1: 0.0331 (6x) | 0.8344 (14x)
row 2: 0.015625 (3x) | 1.0 (17x)
```

Since the caller sorts by this field, the symptom wasn't an unstable number but a **wrong order**:
the entity the query named fell into an arbitrary rank position. `_relevance_score` is exactly the
monotonically decreasing value the engine returns — 0.0331, 0.0167, 0.0161, 0.0159, 0.0156 for
ranks 0 through 4 — while `_score` runs in a different order on this fixture (0.834, 0.883, 0.939,
1.0).

### Rationale — why separate lists instead of normalizing

Hard project rule, recorded in memory and in the ADR: ranking comes from the engine. The 331 lines
of hand-weighted score merging in Go were deliberately removed in T14. This rules out the "obvious"
fix ("normalize both scores with a weight") — that is exactly the Go-side fusion that was removed.

The priority stands: entities first, files second, each list ordered internally on its own terms —
not a ranking policy, just ordering between two response types, which is exactly what the original
comment said. The code was doing the opposite of what it was supposed to.

The prompt's second suggestion — a single table with a `kind` field — was considered and rejected
as unnecessary: it existed to make scores comparable, but the problem was never corpus
comparability. It would cost a schema change, a full rebuild, and incremental-indexing work, to fix
something that wasn't the cause.

## Plan & Task Breakdown

- [x] **T1 — Confirm the prompt's hypothesis against a frozen copy** — done, and it ruled out the
  hypothesis. Measured against the frozen copy, see "The prompt's diagnosis was wrong" above. The
  clues were the ones the prompt pointed at: the entity leads 156.4 to 29.6 in the keyword channel.
  What actually fails is the fused-vs-raw score scale on the vector channel, not corpus size — a
  direct consequence of Real Cause 1 — and it changes the fixture the prompt called for: five files
  are enough, because hundreds beat tens at any corpus size.
- [x] **T2 — Write failing guards for precedence between entities and files** — before
  implementing the fix, per TDD.
- [x] **T3 — Split entity and file lists apart in `SearchIndex.search`** —
  `internal/ast/search_lance.go`.
- [x] **T3b — Make sure the opt-in reranker also understands this precedence** — mid-task request
  from the engineer. See "The reranker" below.
- [x] **T4 — Empty-index guard on the `ast index` shortcut path** — `internal/ast/pipeline.go`,
  symmetric to the existing guard for the graph.
- [x] **T5 — Test for T4** — `internal/ast/search_index_missing_test.go`, covering two on-disk
  states.
- [x] **T6 — Confirm no gate regressed** — `TestSearchIndexQualityFloor` 11/11 + 5/5; nothing
  regressed, which was the bar to clear.
- [ ] **T7 — Verify via CLI with the payload binary**, against the real store and a real embedder.
  Output below.
- [ ] **T8 — Documentation** — two new spec sections, this log, and memory.

## Implementation Details

### `internal/ast/search_lance.go` — `SearchIndex.search`

Before: query entities, query files, **concatenate and sort everything together** by
`RelevanceScore`, then trim to `topK`.

After: sort the entity list among itself, sort the file list among itself, concatenate
entities-first, then trim to `topK`. No score comparison between the two lists.

Sorting by score still applies **within each list** — that's what gives it a deterministic order
across rebuilds — and it became safe again precisely because the score column is now the correct
one.

### `internal/lancestore/store_lancedb.go` — building the hit

`Hit.Score` now always exposes the fused score when fusion happened, and the raw score when it
didn't. Two new fields, `Hit.RawScore` and `Hit.RelevanceScore`, are documented in `lancestore.go`.

### `internal/ast/pipeline.go` — the shortcut guard

The `SearchIndexBuilt` guard already existed, with a comment describing this very defect. It
covered the graph's half of the store, not the search half: when nothing changed, it still checked
`SearchIndexBuilt` — which counts rows, because `OpenSearchIndex` creates whatever it opens, so the
directory exists exactly in the broken case — and if it's empty, it replays the shards via
`BuildSearchIndexFor`, without reparsing: the parse cache is by definition current on this branch.

A new field, `PipelineResult.SearchIndexRebuilt`, lets the CLI report **which** half it fixed
instead of reusing the graph's rebuild message.

### The reranker

Explicit mid-task request from the engineer: "does the opt-in reranking mechanism also respect this
precedence?" Checked:

The reranker is applied per-table inside `search()`, and no production caller enables it —
`Rerank` is only set in tests. The file-side query was built by hand and didn't forward the
`Rerank` option. So if someone did enable the reranker, the entity list would end up scored by the
cross-encoder while the file list stayed on raw BM25 — the same scale defect, wearing a third
number.

Fixed by propagating `Rerank` to the file pass too, with a guard so the reranker is reached by
**both** passes.

Worth noting: even with the reranker on, precedence still holds unconditionally — entities first,
files second. The cross-encoder scores the (query, candidate) pair, and those scores **are**
comparable across the two lists, so merging them would become defensible. But that's a decision to
make based on measurement, not on a path nobody currently exercises: the reranker being factored
out is just an embarkation point without measurement backing it, on a path no one follows today.

## Use Cases

### UC-01 — Hybrid search by entity name

- **Actor**: an agent via MCP, or a person via `graphit ast query --hybrid`.
- **Preconditions**: index built; embeddings present (otherwise degrades to keyword-only).
- **Main Flow**:
  1. `QueryService.HybridSearch` embeds the query and calls `SearchIndex.HybridSearch`.
  2. `search()` queries `entities` with text + vector; the engine fuses them and returns the fused
     score.
  3. The entity list is sorted by `_relevance_score` (the fused column), with ties broken by
     identity.
  4. If `len(entities) < topK || topK <= 0`, queries `files` with text only, ordered among
     themselves.
  5. Concatenates entities + files, trims to `topK`.
- **Alternative Flows**:
  - vector absent → falls back to `Search`, keyword-only on both passes.
  - entity list already fills the quota → the file pass doesn't run.
  - `Rerank` enabled → both passes go through the cross-encoder.
- **Error Scenarios**: the file pass fails → the response still returns with entities only, gated
  by `ferr == nil` — a deliberate degradation. A failure in the entity pass propagates.
- **Postconditions**: entities before files; each list sorted by its own score.
- **Affected Files**: `internal/ast/search_lance.go`, `internal/ast/query.go`,
  `internal/lancestore/store_lancedb.go`.

### UC-02 — `ast index` on a store whose search index is empty

- **Actor**: a person who swapped binary/daemon versions.
- **Preconditions**: parse cache current; graph present; `search.lance` absent or has no rows.
- **Main Flow**:
  1. The pipeline compares hashes and concludes nothing changed.
  2. Before taking the shortcut, it checks `SearchIndexBuilt(ctx, dbPath)` — which counts rows.
  3. Empty → `BuildSearchIndexFor` replays the shards; `SearchIndexRebuilt = true`.
  4. The CLI reports `N files up to date; search index was empty and was rebuilt from cache`.
- **Alternative Flows**: populated index → normal flow, message unchanged.
- **Error Scenarios**: a rebuild failure returns an error (`rebuild search index from cache: …`)
  instead of reporting success — that was the whole point.
- **Postconditions**: the index is populated and search works, without reparsing.
- **Affected Files**: `internal/ast/pipeline.go`, `cmd/graphit/commands/runners.go`.

## Test Cases & Acceptance Criteria

### Feature: Precedence Between the Two Passes
Ref: UC-01

#### Scenario: A query naming a method does not return a file first
```gherkin
Given an index with one file declaring "evictOldestStaged" and four that only mention it in prose
And all entities have embeddings, with the target's embedding aligned to the query vector
When running a hybrid search for "evictOldestStaged" with topK = 0
Then the rank-1 result is not of type File
  And the rank-1 result is named "evictOldestStaged"
```

#### Scenario: Precedence Does Not Equal Exclusion
```gherkin
Given the same index
When running a hybrid search for "evictOldestStaged" with topK = 0
Then there is at least one result of type File
  And no File appears before an entity
```

#### Scenario: topK spends its quota on entities
```gherkin
Given the same index
When running a hybrid search for "evictOldestStaged" with topK = 2
Then only two results are returned
And none of them is of type File
```

#### Scenario: The keyword channel did not change
```gherkin
Given the same index, without embeddings
When keyword-searching for "evictOldestStaged"
Then the rank-1 result is named "evictOldestStaged"
```

### Feature: The Two Score Columns
Ref: UC-01

#### Scenario: The same hybrid query always returns the same score
```gherkin
Given a populated table and an unchanged index
When the same hybrid query runs 20 times
Then every row returns exactly one consistent score value
```

#### Scenario: The score agrees with the engine's order
```gherkin
Given a populated table
When a hybrid query returns at least two rows
Then the score is monotonically non-increasing in the order returned
And Hit.Score equals Hit.RelevanceScore
```

#### Scenario: Ranking Without Fusion Falls Back to the Raw Score
```gherkin
Given a populated table
When only a text-only query runs
Then Hit.Score equals Hit.RawScore
And Hit.RelevanceScore is zero
```

### Feature: The Empty-Index Guard
Ref: UC-02

#### Scenario Outline: An absent or empty index is reconstructed
```gherkin
Given an indexed project, with a current parse cache and the graph present
When the search.lance directory is left in state "<state>"
And the pipeline runs again with no file changes
Then no files are reparsed
And SearchIndexRebuilt is true
And SearchIndexBuilt becomes true
And a search for the fixture's term returns results

Examples:
  | state                     |
  | removed entirely          |
  | exists but empty (mkdir)  |
```

### Feature: The Reranker Reaches Both Passes
Ref: UC-01

#### Scenario: Both passes go through the reranker
```gherkin
Given a populated index and a Reranker that counts calls
When search runs with Rerank configured and topK = 0
Then the reranker was called at least 2 times
```

### Feature: Hybrid Path Determinism
Ref: UC-01

#### Scenario: Top-1 and Stable Set Between Rebuilds
```gherkin
Given the same corpus built eight times, each time with a fresh shard cache
When the same hybrid query runs against each build
Then the rank-1 result is the same across all of them
And the result set is the same across all of them
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/search_lance.go` | Modified | precedence between passes; propagates `Rerank` to the file pass; comment with the measurement |
| `internal/lancestore/store_lancedb.go` | Modified | separates `_score` from `_relevance_score` when building the hit |
| `internal/lancestore/lancestore.go` | Modified | `Hit.RawScore` and `Hit.RelevanceScore` fields, documented |
| `internal/ast/pipeline.go` | Modified | `SearchIndexBuilt` guard on the "nothing changed" shortcut; `SearchIndexRebuilt` field |
| `cmd/graphit/commands/runners.go` | Modified | new message for "search index rebuilt from cache" |
| `internal/ast/search_scale_test.go` | Created | guards for precedence, topK quota, reranker reaching both passes, and rebuild stability |
| `internal/ast/search_determinism_test.go` | Created | guards for deterministic ordering on the keyword and hybrid paths |
| `internal/ast/search_index_missing_test.go` | Created | guard for two states of an empty index (removed vs. present-but-empty) |
| `internal/lancestore/hybrid_score_columns_test.go` | Created | stability of the score, and agreement with the engine's order |
| `internal/ast/measure_real_store_test.go` | Created | a measurement harness against a frozen real store (skipped by default) |
| `docs/specs/ast_module.md` | Modified | two new sections: precedence between passes, and the shortcut checked on both halves |

### Verification via CLI (T7), with the payload binary

`make install`, against the real store (61,446 entities) and with a real embedder
(`daemon-embedder (proxy→daemon)`):

```
$ graphit ast query --hybrid "evictOldestStaged"
  0  Method     0.03333  evictOldestStaged            internal/hub/s3_store.go:536
  1  Comment    0.03279  evictOldestStaged drops…     internal/hub/s3_store.go:535
  2  Function   0.02976  TestStagedEventsAreBounded   internal/hub/s3_store_test.go:358
  3  Method     0.02912  …
  …
 19  Function   0.01429  …
 20  File       29.633   internal/ast/search_lance_test.go
 21  File       24.367   internal/hub/s3_store.go
  …
```

Entities 0–19 with strictly decreasing RRF, files from 20 onward with decreasing BM25. The
requested method ranks 1, with `Type` labeled `Method`, not `File`.

```
$ graphit ast index          # in a store whose search.lance was removed
✓ 1 files up to date; search index was empty and was rebuilt from cache (0.1s)
  › Timing: discover 0.00s, hash 0.00s, parse 0.00s, write 0.06s
$ graphit ast query --hybrid "EvictOldestThing"
    "Name": "EvictOldestThing"
```

Done in a scratch project (`/tmp/t2-demo`), not the real store.

## Trade-offs & Decisions

- **Precedence over normalization.** Normalizing both scales in Go would be equivalent to the
  weighted merge that T14 turned off. Precedence between two response types is not a ranking
  policy.
- **A single table with a `kind` field, discarded** — it would solve corpus-comparability issues,
  which were never the cause. It would cost a schema change, a rebuild, and incremental-indexing
  work, for nothing.
- **Sorting by score still applies**, within each list, because the column is now the right one. An
  alternative I initially tried was to preserve the engine's order and not sort the hybrid result
  at all — I **reverted** that once Real Cause 2 surfaced: with the right column, sorting by score
  *agrees* with the engine and still adds a deterministic tiebreaker, which the engine doesn't
  provide on its own.
- **Reranker: propagate it, don't merge it.** See "The reranker" above.

## Technical Debt

- [ ] **Tie order among rows on both channels shuffles across rebuilds.** The keyword channel
  breaks ties by identity, and on the hybrid channel the engine's rows end up with distinct RRF
  values (differing only in the fourth decimal place, since it has to pick an order), so the tie
  never actually occurs in practice. Top-1 and the result set are stable — verified by rebuilding
  eight times, since there is no identifiable internal row order. Fixing this for real would mean
  deciding, in Go, that two of the engine's rankings are "close enough to call a tie," which is a
  ranking-policy decision. Recorded in `TestHybridTopResultAndSetAreStableAcrossRebuilds` and in the
  spec.
- [ ] **`TestHybridSearchOrderIsDeterministic` doesn't actually test the hybrid path.** It calls
  `HybridSearch(..., nil, 10)` — a nil vector — so it degrades to the keyword path and never
  exercises fusion. Not fixed in this pass, because the new gates cover the real path, but the name
  is misleading.
- [ ] **`TestHybridSearchQualityFloor` was skipped — resolved in the second session**, see the
  section at the end of this log. The first time it actually ran, it measured **0 of 11** decisive
  probes on the commit before this fix, confirming the `_score`/`_relevance_score` defect after the
  fact. With the fix, 11 of 11.
- [ ] **The file pass swallows its own error** (`if ferr == nil`). A failure returns a response
  with entities only, indistinguishable from a corpus that legitimately has no matching files.
  Pre-existing, not touched.

## System Knowledge

- **`--top` defaults to 0, and passing any value runs the same two-pass path** — the default
  invocation always runs both passes. This defect was 100% reproducible via the CLI and invisible
  to tests that passed an explicit `topK`.
- **The entity score beats the file score in BM25 for two compounding reasons:** the entity is a
  short document (BM25 normalizes by length and rewards that), and the term is rare across the
  large corpus (high IDF). The "large-corpus dilution" intuition is wrong here.
- **`_relevance_score` is the RRF sum** (`~1/(60+rank)`), which is monotonic with the engine's
  order. `_score` is the text channel's raw value and carries no ordering guarantee in a hybrid
  query.
- **A pure-semantic row has no score at all — only `_distance`.** So `Hit.Score` is 0 in
  `mode: semantic`, and `confidentSemanticResults` filters on `RelevanceScore < semanticFloorCosine`
  using the mapped `Distance` field instead. Not touched here.
- **Careful when writing a fixture with vectors:** giving every entity the **same** embedding makes
  the vector channel pure noise with maximum confidence, so the engine's own BM25-based ranking
  lets an irrelevant entity take rank 1. The first fixture did exactly that and the test **flaked**
  — passing alone, failing in the full suite, because it pointed in one direction while the rest of
  the suite pointed in another.
- **The wiki doesn't have this class of defect**: it only ever queries a single table (`chunks`),
  so there's no second column to collide with. Verified, not assumed.
- **The intermittent segfault under memory pressure** (buffer pool with no cross-process
  coordination) has no rollback here — it's a known, already-tracked backlog item, and out of scope
  for this session.

## Progress Log

### 2026-08-24

Opened before touching anything. Read `hub-on-s3-icebug-and-lancedb.md` (continues T15), the backlog
prompt, and the `lancedb`/search memories — in particular the fix that brought the floor from 13/16
to 11/11 + 5/5, and the T14 decision banning score merges in Go.
- Froze a copy of the real store and measured both passes. The prompt's diagnosis fell apart.
  Recorded above; the finding was worth more than a fix would have been, exactly as the original
  prompt hinted.
- Reproduced the exact report with `topK = 0`: the same four files, in the same order.
- Mid-task engineering request: "does the opt-in reranking mechanism also respect this precedence?"
  This became T3b. I initially suspected the file query was crashing on the same scale defect
  wearing a third number.
- T2/T3: wrote failing guards; implemented precedence; guards now pass.
- Running the full suite with everything checked, the new test failed a different way: the order
  within the entity list was wrong. Two causes, in this order: (a) a fixture with a shared
  embedding — my mistake, fixed; (b) `_score` and `_relevance_score` collapsed by map iteration — a
  real defect in `internal/lancestore`, isolated with 20 identical queries against an unchanged
  index.
- I initially tried "don't sort the hybrid result, keep the engine's order," then reversed that
  once I found Real Cause 2: with the right column, sorting by score agrees with the engine and
  still breaks ties.
- A determinism gate I had written failed; investigating showed **my own spec was too strict** —
  tied rows on both channels permute. Rewrote it to assert what actually holds (top-1 and the
  result set), with the residual risk logged in Technical Debt and in the spec.
- T4/T5: added the `SearchIndexBuilt`-style guard for the search index, symmetric to the graph's.
  Verified both tests fail with the guard off and pass with it on.
- T6: `TestSearchIndexQualityFloor` 11/11 + 5/5, `TestTruncatedQueryCoverage` 8/8 + recall. Nothing
  regressed.
- T7: `make install` and CLI verification, both defects. Output above.
- T8: two new spec sections; this log; memory.
- Suites: green. `go vet` warnings are confined to an already-present ANTLR-generated parser.

---

## SECOND SESSION (2026-08-24): the missing `onnxruntime.so`, and the two defects the skip was hiding

Request from the engineer: "fix this issue of onnxruntime.so not being present, and test it" — a
technical-debt item logged above: the ORT-dependent gates were being skipped on this machine.

### Why ORT was never found outside the payload

`findORTLibrary` looks in two places: next to the executable, and on the loader path. The library
actually lives just outside the executable — inside the launcher's extracted payload — so:

- a `go test` binary lives in a temp directory from the toolchain → not found;
- a `go build` output binary has no payload next to it → not found;
- and nothing set the loader path.

Without finding it, `initONNXRuntime` was never called and the binding fell back to its default
library name — the system's `onnxruntime.so`, not the version this project ships. Hence the
reported error message.

Two fixes, resolving different things:

1. **`findORTLibrary` now also looks in the extracted payload** (`brand.RuntimeDir(version.Version)`),
   the same resolution the AST module already uses for grammar YAMLs (`runtimeQueriesDir`) and that
   the Hub store uses for extensions (`ExtensionDir`). This fixes the local binary, **not** the
   tests — under test, `HOME` is isolated by `internal/brand/testhome.go`, so the runtime path
   points at a throwaway, empty home. That's correct and deliberate: this isolation exists
   precisely so tests never read the real home. It wasn't worked around.
2. **`make test` puts the ORT cache on the loader path from the Makefile** — `ORT_HOST_FETCH`/
   `ORT_HOST_LIB` per `GOOS`, via `LD_LIBRARY_PATH` and `DYLD_LIBRARY_PATH`. This is what makes the
   gates actually **run**.

### What the skip was hiding — two defects, one of them serious

With ORT reachable, two gates that had been reporting **skip** without ever running started to
run — and **both failed**. I took a baseline in a worktree on the previous commit (`abef386`) to
separate what was mine from what was pre-existing:

| gate | on baseline `abef386` | after |
|---|---|---|
| `TestHybridSearchQualityFloor` | **0 of 11** decisive probes | **11 of 11** |
| `TestSearchIndexSemantic` | `SemanticSearch` returned NOTHING | passes |

The first is retroactive confirmation of the `_score`/`_relevance_score` defect: on the baseline,
several probes returned the wrong entity outright — not a weak ranking, an **absent** one, exactly
what map iteration produces. The first session's fix took the hybrid channel from 0/11 to 11/11,
even ahead of the keyword-only baseline (10/11) — and that only became visible once the gate
started actually running.

The second is a new, pre-existing, and serious defect: `SemanticSearch` never returned anything. A
pure vector query returns `_distance` and no score column, so `RelevanceScore` came out zero;
`confidentSemanticResults` compares that field against the floor, found it below on the very first
result, and truncated the whole list to empty. The old SQLite-backed index used to compute the
cosine in Go and write it into that field itself; the port to the new engine brought over the query
but left the calculation behind.

Fixed by deriving the cosine from the distance. The metric was measured, not assumed, against unit
vectors with a known cosine:

```
cosine 1.000 -> distance 0.000
cosine 0.707 -> distance 0.586
cosine 0.500 -> distance 1.000
cosine 0.000 -> distance 2.000
```

That is, `d = 2 - 2cos`, so `cos = 1 - d/2` — exact, not approximate, because the embedder always
returns L2-normalized vectors. No metric is configured on the index, so this is the engine's
default choice — `TestVectorMetricIsSquaredL2OnUnitVectors` exists to fail if a version bump ever
changes it.

### The 132 MB download that skip was also hiding

`NewLocalEmbeddingClient` called `EnsureModel` before `initONNXRuntime`. On a machine without the
runtime, it downloaded 132 MB and only then discovered it couldn't use it — the cache was no
defense against the failure, so the next call paid the download again. The cache path is derived
from `HOME`, and each test binary gets its own throwaway one: measured on this machine, **29
abandoned homes, 4.3 gigabytes**, sitting in a tmpfs.

This closes the missing half of the 2026-08-07 task
(`docs/tasks/make-test-slowness-measured-and-132mb-download-hidden-in-tests.md`), which
covered the reranker's model cache but not the embedder's — where four test files construct an
embedding client and none of them seed the cache.

Two fixes:
- **ORT first, model second.** A test without ORT now skips in 0.00s with an empty cache.
- **`<BRAND>_MODEL_CACHE` points at the root models directory**, shared across test binaries;
  `make test` sets it too. It has to be the root, not a leaf — that's load-bearing, because the
  reranker resolves its own path from that same root. My first attempt overwrote a leaf directory
  instead: it moved one model and left the reranker pointing at the real home — the first attempt
  overwrote the leaf and the reranker fell back to `/tmp/bge-reranker-base`, where an unrelated
  test had already run and written stubs there. Fixed by removing the `filepath.Dir(base)` call
  from `NewRerankModelManager`, which was the fragility that allowed this.

The test suite's own `HOME` isolation almost completely hid this: `internal/ai/main_test.go`
asserts on model paths against an override that points at a shared directory, and the defect
happened to point in the direction that let the test pass while it was measuring the wrong
filesystem.

### A fix that tightens a lock rather than loosening it — why was the first attempt wrong?

`TestHybridSearchQualityFloor` had a `tie string` field, able to name **one** defensible alternative
answer. The probe for "configuration" has **seven** equally plausible entities in this corpus
(`parseConfig`, `Config`, `loadUserConfig`, `coreConf`, `CONF_MGR`, `configLoader`,
`initConfiguration`) — one of them with the literal docstring *"Configuration for the parser."* The
field became `ambiguous []string`.

My first instinct was to swap the ambiguous probe for recall@5 — wrong. Measured: it failed,
because seven defensible answers and a five-slot window for one query only moves the tie-breaker
from position 1 to position 5 — and it still failed on a result set that was **good** (five
`configuration`-related entities), just not containing the one the fixture happened to name first.
The original author's intent was correct; what was wrong was only the size of the list. Reverted to
"the winner has to be one of the defensible answers," now with the complete list.

The gates that were already passing stayed where they were: `TestSearchIndexQualityFloor` 11/11 +
5/5, `TestTruncatedQueryCoverage` 8/8 + recall.

### Verification

- `make test` exits **0** across 47 packages, with **no** abandoned model copies — down from 4.3 GB
  to **740 KB**.
- The three new/renovated gates pass **under `-race`**, which is what `make test` uses.
- `TestInitONNXRuntime` now runs in **1.35 s**, versus ~30 s before — the difference was the
  download.
- Without ORT on the loader path: it skips in 0.00s and the model cache stays **empty**.

### Files Changed in This Session

| File | Change | Reason |
|---|---|---|
| `internal/ai/embedding_local.go` | Modified | look in the extracted payload; ORT before model |
| `internal/ai/model_manager.go` | Modified | `ModelsDir()` with override `<BRAND>_MODEL_CACHE`; `ModelCacheDir` derived from it |
| `internal/ai/rerank_model.go` | Modified | resolve from the root, not from `filepath.Dir(ModelCacheDir())` |
| `internal/ai/main_test.go` | Created | clears the override in this package, which asserts on paths |
| `internal/ai/ai_test.go` | Modified | expected failure point moves from "ONNX init" to "load tokenizer" now that the runtime is found |
| `internal/ast/search_common.go` | Modified | derives the cosine from the distance |
| `internal/ast/search_lance.go` | Modified | `SemanticSearch` now derives `RelevanceScore` via `cosineFromSquaredL2`, with the measurement above |
| `internal/ast/search_hybrid_floor_test.go` | Modified | `tie string` → `ambiguous []string`; records what the gate measured on each run |
| `internal/ast/vector_metric_test.go` | Created | pins the engine's distance metric and `SemanticSearch`'s return value |
| `Makefile` | Modified | `ORT_HOST_*`, `MODEL_CACHE`, `BRAND_ENV`; `test` depends on `fetch` and exports all three |
| `docs/specs/ast_module.md` | Modified | new sections on the vector metric and on running the tests that require an embedder |

### Technical Debt for this session

- **`HOME` isolation from the payload makes fix (1) untestable via ORT directly** — it covers the
  local binary, not the test binaries, which still rely on `make test` to place the loader path. A
  bare `go test` still skips those gates. Alternative not taken: a `TestMain` in `internal/ast`
  that sets the loader path on its own — which would mean a test reaching into the process's own
  load environment.
- **Nothing raises the alarm in CI when a gate skips.** A gate that skips still reports success,
  which is the trap this whole session fell into. Arguably CI should fail if an essential gate is
  skipped; not done here, because it requires deciding what counts as essential.
