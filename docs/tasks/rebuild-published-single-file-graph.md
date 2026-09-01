# Task: the rebuild published a single-file graph on top of a complete one

**Status: completed** on 2026-08-05. Fifth in the series, and the only one with real data loss.

## The question that opened the case

The Engineer asked whether the read database should not stay 100% functional until the swap took
over. It should — and it **does**: `json_rebuild.go` and `incremental_rebuild.go` build in a
temporary and only then call `AtomicSwapDB`, with an explicit scar in the code against publishing a
half-loaded database. My first answer blamed `--reset`, which erases the whole directory before
indexing. He corrected me: **he had not passed `--reset` when the error happened.**

## What the log showed

```
17:48:38  strategy selected  type=full-rebuild  files=1
17:48:38  cache loaded       files=1
17:48:38  COPY complete      nodes=1  edges=0
17:48:38  swapping DB        mode=atomic          ← publicado
...
18:46:06  strategy selected  type=incremental     total=2
```

Four times that day (17:48, 18:43, 18:45:24, 18:45:59). The `total=2` that follows is literally the
`CLAUDE.md` + `__config__` that I had seen when querying the graph.

Before getting there I misread a line and said a rebuild had published 36 nodes: `nodeCount`
counts **COPY batches**, not rows (`json_rebuild.go:121`), so `nodes=36 edges=80` is a healthy
rebuild of 828 files. The real proof is `nodes=1 edges=0`.

## The chain

1. Someone bumps `shardCacheVersion` — me to 2, the other session to 3 and 4, on the same day. The
   manifest is discarded **by design**, and `jsonCache.Count()` goes to 0.
2. The daemon sees a file change and calls `RunPipelineForPaths` for that file only. **Scoped**
   mode: there is no discovery, because the watcher already knows what changed.
3. That file is parsed, and the cache comes to hold **1**.
4. `pipeline.go:539` decides the strategy:
   ```go
   useIncremental := ... && (len(changedFiles)+len(deletedFiles)) < jsonCache.Count()
   ```
   `1 < 1` is false → **full rebuild**.
5. `RebuildFromJSON` assembles the entire graph from the cache it has — one file — and the swap
   publishes it. The 828-file graph becomes 1.

The comparison is a **cost** heuristic being used as a **correctness** decision. With an empty cache
it silently means "rebuild everything from nothing". And the swap, which works, publishes that
successfully: no error anywhere.

## The two layers

**1. A scoped run does not trust an empty cache** (`pipeline.go`). If the caller named the paths and
the cache is empty, the premise of scoped mode is dead — it falls back to full discovery. It costs
one slow pass; the alternative cost the graph.

**2. Shrink guard before the swap** (`json_rebuild.go`). Sibling to the guard against COPY with
errors, for the failure that one does not see: every COPY worked, the new database is internally
consistent, and it is still wrong. Above 20 live files, losing more than half at once refuses
publication and keeps the old database — stale is visible and recoverable, empty reads as "this code
does not exist". The way out is a full index (`--reindex`/`--reset`), which is how a real removal of
half the project gets published.

## The defect I introduced myself, and the test that caught it

The first version of the guard counted live files through `lb.Query` — the **production** backend.
That leaves a connection open on exactly the file `AtomicSwapDB` is going to rename, and the swap
started failing: the integration test reported an empty graph after an index that reported
`parsed=40 errors=0`. The count is now done beforehand, through a read-only handle that closes right
after (`countLiveFiles`).

## Tests

- `TestFullRebuildRefusesToPublishAGraphThatLostMostOfItsFiles` — indexes 40 files, throws the cache
  away, rebuilds from one, and requires that publication be refused **and** that the live graph
  still holds the 40.
- `TestScopedRunWithAnEmptyCacheRediscoversTheProject` — the layer above: `RunPipelineForPaths`
  with one file and an empty cache has to rediscover the whole project.
- `TestShrinkGuardStaysOutOfTheWay` — the five situations in which the guard must not fire (no live
  graph, below the floor, ordinary edit, real removal of a few, exactly at the ratio), plus the one
  in which it has to.

## Noted

One run of the full suite failed without printing a failure line, and did not reproduce in four
subsequent runs. I do not know what it was; it is recorded instead of rounded up to green.
`stageGrammar` touches a global extension map, and there are now more tests using it alongside
parallel tests — that is the first place I would look.
