Task: The rebuild published a graph over a complete one

Status completed as of August 5, 2026. Fifth in the series, and the only one with actual data loss.

The question that opened the case

The Engineer asked if the reading database should not be 100% functional until the swap took over.
Should — and **it stays**: _INLINE_0_ and _INLINE_1_ construct in a temporary and only then call _INLINE_2_, with an explicit code mark against publishing half-loaded reading databases. My first response blamed _INLINE_3_, which wipes out the entire directory before indexing. He corrected: **he hadn't given _INLINE_4__ when the error occurred.**

What the log showed

```
17:48:38  strategy selected  type=full-rebuild  files=1
17:48:38  cache loaded       files=1
17:48:38  COPY complete      nodes=1  edges=0
17:48:38  swapping DB        mode=atomic          ← publicado
...
18:46:06  strategy selected  type=incremental     total=2
```

Four times today (17:48, 18:43, 18:45:24, 18:45:59). The following `total=2` is literally the `CLAUDE.md` + `__config__` that I had seen when consulting the graph.

Before I got there, I read an incorrect line and said that a rebuild had published 36 lots: **__INLINE_8__** counts **lots of COPY**, not lines (___INLINE_9__), so ___INLINE_10__ is a healthy rebuild.
The real test is ___INLINE_11__.

## A cadeia

Sure! Here is the translation of your Portuguese text into idiomatic English:

1. Someone bumps `shardCacheVersion` — I go to 2, they go to 3 and 4 on the same day. The manifest is discarded **by design**, and `jsonCache.Count()` goes to 0.
2. The daemon sees an file change and calls `RunPipelineForPaths` just for it. Mode **scrobbled**: there's no discovery, because the watcher already knows what changed.
3. That file is parsed, and the cache passes to have **1**.
4. `pipeline.go:539` decides the strategy:
   ```go
   useIncremental := ... && (len(changedFiles)+len(deletedFiles)) < jsonCache.Count()
   ```
\_INLINE\_16\_\_ is false → **full rebuild**.
5. \_INLINE\_17\_\_ builds the entire graph from the cache that has — an archive — and the swap
   publishes. The graph of 828 files becomes 1.

The comparison is used as a heuristic for **cost**, serving as the decision to **correct**. With an empty cache, it silently means "rebuild everything from scratch." And the swap, which works, publishes this successfully: no error in any place.

## As duas camadas

Scoping execution does not trust an empty cache (`pipeline.go`). If the caller names the paths and the cache is empty, the premise of the scoped mode dies — it falls to complete discovery. It costs a slow pass; the alternative costs the graph.

**Guardian of Shrinkage Before the Swap** (`json_rebuild.go`). Brother to the Copy Guardian with an error,
for the failure that it does not see: all Copies functioned, the new database is internally consistent,
and yet it still remains incorrect. Above 20 live files, losing more than half at once refuses publication
and keeps the old database — obsolete is visible and recoverable, empty reads as "this code doesn't exist". The solution is a complete index (`--reindex`/`--reset`), which is like a real removal of half of the project upon publication.

The defect I myself introduced, and the test that caught me

The first version of the guard counted live files by `lb.Query` — the backend **for production**.
This leaves an open connection exactly over the file that the `AtomicSwapDB` will rename, and swap has fallen: the integration test reported a graph with zero nodes after an index that reported `parsed=40 errors=0`. The count is now being done before, using a read-only handle that closes immediately (`countLiveFiles`).

## Testes

- `TestFullRebuildRefusesToPublishAGraphThatLostMostOfItsFiles` — indexes 40 files, clears the cache
  out, reconstructs from one, and requires that publication be refused **and** that the live graph continues with the 40.
- `TestScopedRunWithAnEmptyCacheRediscoversTheProject` — the top layer: `RunPipelineForPaths`
  with an empty file and cache must rediscover the entire project.
- `TestShrinkGuardStaysOutOfTheWay` — five situations in which the guard cannot fire (without a live graph, below the floor, common edit, real removal of few, exactly at the ratio), plus that needs to fire.

## Anotado

A complete suite execution failed without printing a failure line and did not reproduce in four subsequent executions. I do not know what happened; it is registered instead of rounded to green.
`stageGrammar` now manipulates a global extension map, and this is where there are more tests being used together with parallel tests — that's the first place I would look.
