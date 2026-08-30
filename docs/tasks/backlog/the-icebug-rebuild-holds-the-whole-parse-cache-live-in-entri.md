# The icebug rebuild still holds the whole corpus live — ~14.6 GB for a 120k-file repository

> **Re-scoped 2026-08-30 after the work in
> `docs/tasks/the-parse-cache-stops-paying-for-the-same-string-twice.md`.**
> Two of the three items this brief originally carried are DONE, and the third was described
> wrongly. What is below is what actually remains.

## Done, do not redo

- **The memory limit came off MemTotal.** REMOVED entirely, on the engineer's decision: a
  soft limit indicates the real term is not under control, and its implementation was
  Linux-only in a product that must run on Linux, Windows and macOS. Do not reintroduce one
  without asking. `internal/ast/export_memlimit.go` is deleted.
- **The parse cache paid one allocation per occurrence of every repeated string.** Fixed by
  interning at shard adoption: **26.0 GB → 14.6 GB projected** for the 120,064-file corpus.

## The correction that matters for whoever picks this up

The original brief said the fix was to stop `rebuildIcebugFromCacheWithDelta` retaining its
`entries map[string]*parseCacheEntry`. **That is wrong and will waste your time.** `ri.scan()`
appends every entry into `ri.fileEntries`, which is what the row producers iterate. Both hold
the SAME `*parseCacheEntry` pointers, so dropping the `entries` map frees the map headers —
single-digit MB — and none of the corpus.

## The consequence to keep in mind

With no limit, peak heap is `2 x live` with nothing bounding it — ~29 GB for this corpus,
which fits the 61 GiB machine that was being killed at 46 GiB. A corpus roughly TWICE this one
(around 75 M graph elements) is killed again, and the fix for that is the work below, not
another limit.

## What remains

The rebuild's live heap is still O(corpus): ~14.6 GB for 120,064 files / ~37.5 M graph
elements, measured with `TestShardCacheFootprint`. The retention is structural, not a stray
reference: `exportDirectWithReverse` makes one pass over `ri.fileEntries` per node label and
one per relationship type, so the corpus has to be either resident or re-readable.

Two directions, neither cheap:

1. **Re-stream the shards per pass.** ~30 passes over 21 GB of JSON. Bounds memory at one
   file, and trades it for I/O and decode time that is likely worse than the current run.
2. **Reorganise the export around a single pass**, accumulating every label's and every
   relationship's columns together. Bounds memory at the OUTPUT rather than the input, which
   is the right shape — the finished bundle is tens of MB — but it is a rewrite of
   `exportDirectWithReverse` and has to preserve the load-bearing generation ORDER that stub
   emission depends on (see the T3 constraint in
   `docs/tasks/icebug-export-streams-instead-of-materializing.md`).

Smaller, independent, and much cheaper:

- `ShardCache.Store` does not compact, so the PARSE phase still pays one allocation per
  occurrence. Bounded by the flush interval rather than by the corpus, so it is not the OOM
  path — but the interner is already on the cache and would apply.
- `entity.UID` is 61.8 MB of 129.8 MB of entity string bytes on a 2,000-file sample and is
  almost entirely distinct, so interning cannot touch it. It is largely `path + ":" + name`,
  i.e. derivable rather than stored.
- `ri.decls map[string][]declRef` and `ri.entityUIDs map[string]string` both scale with the
  entity count and are untouched.

## The acceptance criterion, whichever direction is taken

Peak live heap during the rebuild stops scaling with `cache.Count()`. `TestShardCacheFootprint`
(env-guarded, in `internal/ast/shard_cache_footprint_probe_test.go`) is how to measure it
against a real store without loading the whole thing.
