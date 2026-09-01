BuildDBFromCache returns 0 chunks on a memory wiki with 455 shards.

Observed on August 18, 2026 during the correction of the mixed vector lot (docs/tasks/wiki-empty-index-due-to-mixed-vector-batch.md), not investigated.

Sonda: spin around the real memory worktree of the project and then run in the same directory. The generation writes 152 pages and 455 shards, and the index is correct (30 MB), but `BuildDBFromCache` responds `chunks=0, err=nil` — that is, `cache.LoadAllChunks()` cannot find anything where `ExportShards` just wrote.

Why matters: `BuildDBFromCache` is like a published wiki that's built into the consumer— the producer compiles once and sends shards, and the consumer builds an index without having the sources. If the format that `ExportShards` writes and what `LoadAllChunks` reads are not the same, a Hub-installed wiki will be empty. And the very function contract says zero is "the shards did not carry any usable content," which is a legitimate response— so this divergence does not raise an error anywhere.

It does not affect memory today: no wiki of memory is installed from shards; memory always compiles from the worktree.

What to do:
Compare what `ExportShards` writes in `<wikiDir>/shards/` (there are three sidecars per page: `.meta.json`, `.wiki.json`, and `.emb.json`) with what `WikiProcessCache.LoadAllChunks()` is looking for. Confirm if the Hub's path (the knowledge installed as an artifact) is actually functioning — the knowledge wikis on this machine have 118 KB with 3 pages, which could be exactly this defect or could be their normal size.
